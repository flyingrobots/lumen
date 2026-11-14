//! Publishing the PNG: the destination holds the whole new image or is untouched.
//!
//! The image is written to a uniquely named temporary file in the destination's directory, the
//! same file system, so the final rename is atomic. The order is the one the Rust standard
//! (section 13.4) asks for:
//!
//! 1. create the temporary file exclusively (`create_new`), so it can neither clobber another
//!    file nor be shared with a concurrent run;
//! 2. write the PNG and flush the user-space buffer;
//! 3. `sync_all` the temporary file;
//! 4. rename it over the destination;
//! 5. on Unix, `sync_all` the containing directory, so the rename survives a crash.
//!
//! Any failure before the rename removes the temporary file and leaves the destination exactly
//! as it was. After the rename the destination is the new image whatever happens next; a failure
//! to sync the directory is reported, not hidden, and does not undo the rename.
//!
//! What touches the file system sits behind [`Filesystem`], so tests can fail a publication at
//! any step on the real encoder's bytes. [`Disk`] is the real one.

use core::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use lumen::{png_len, write_png, PngError, Rgba8Image};

/// How many temporary names to try before giving up. Each try fails only if a file of that name
/// already exists, so reaching the limit takes this many leftover files.
const NAME_TRIES: u32 = 64;

/// How a destination is written, decided from what is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// A regular file or nothing: replace it atomically.
    Replace,
    /// A device, pipe or socket (for example `/dev/null`): write to it, since replacing it would
    /// destroy the node, not its contents.
    InPlace,
}

/// The file system operations a publication needs.
pub(crate) trait Filesystem {
    /// A file being written.
    type File: Write;

    /// Text that makes temporary names differ between concurrent processes (the process id).
    fn unique(&self) -> String;
    /// Whether the destination may be replaced, and how: an error for a directory (or a link to
    /// one) and for a read-only file.
    fn mode(&self, destination: &Path) -> io::Result<Mode>;
    /// Creates a new file, failing with `AlreadyExists` if there is one.
    fn create_new(&self, path: &Path) -> io::Result<Self::File>;
    /// Opens an existing special file for writing, without creating or truncating it.
    fn open_in_place(&self, path: &Path) -> io::Result<Self::File>;
    /// Forces the file's contents to storage.
    fn sync(&self, file: &mut Self::File) -> io::Result<()>;
    /// Renames over `to`, replacing it.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Removes a file.
    fn remove(&self, path: &Path) -> io::Result<()>;
    /// Forces a directory's entries to storage where the platform allows it.
    fn sync_directory(&self, directory: &Path) -> io::Result<()>;
}

/// The real file system.
pub(crate) struct Disk;

impl Filesystem for Disk {
    type File = File;

    fn unique(&self) -> String {
        std::process::id().to_string()
    }

    fn mode(&self, destination: &Path) -> io::Result<Mode> {
        let own = match fs::symlink_metadata(destination) {
            Ok(own) => own,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Mode::Replace),
            Err(error) => return Err(error),
        };
        // A link is replaced itself, never written through: its target is not touched. A link
        // to a directory is refused like a directory, and a dangling one is replaced.
        if own.is_symlink() {
            return match fs::metadata(destination) {
                Ok(target) if target.is_dir() => Err(io::Error::other("is a directory")),
                _ => Ok(Mode::Replace),
            };
        }
        if own.is_dir() {
            Err(io::Error::other("is a directory"))
        } else if !own.is_file() {
            Ok(Mode::InPlace)
        } else if own.permissions().readonly() {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "the file is read-only"))
        } else {
            Ok(Mode::Replace)
        }
    }

    fn create_new(&self, path: &Path) -> io::Result<File> {
        OpenOptions::new().write(true).create_new(true).open(path)
    }

    fn open_in_place(&self, path: &Path) -> io::Result<File> {
        OpenOptions::new().write(true).open(path)
    }

    fn sync(&self, file: &mut File) -> io::Result<()> {
        file.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    #[cfg(unix)]
    fn sync_directory(&self, directory: &Path) -> io::Result<()> {
        File::open(directory)?.sync_all()
    }

    /// Windows cannot open a directory as a file with the standard library, so the entry is
    /// left to NTFS's own journal; other platforms have no directory handle to sync.
    #[cfg(not(unix))]
    fn sync_directory(&self, _directory: &Path) -> io::Result<()> {
        Ok(())
    }
}

/// Why a PNG could not be published.
#[derive(Debug)]
pub(crate) enum PublishError {
    /// The destination cannot be written (no file name, a directory, read-only, a missing
    /// directory) or a file operation failed; the destination is as it was.
    Io(io::Error),
    /// The encoder or the writer failed part-way; the destination is as it was.
    Png(PngError),
    /// The rename happened, so the destination holds the new image, but the directory could not
    /// be synced: the rename may not survive a crash.
    DirectorySync(io::Error),
}

impl fmt::Display for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Png(error) => write!(f, "{error}"),
            Self::DirectorySync(error) => {
                write!(f, "written, but its directory could not be synced: {error}")
            }
        }
    }
}

impl std::error::Error for PublishError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::DirectorySync(error) => Some(error),
            Self::Png(error) => Some(error),
        }
    }
}

/// The directory a destination's temporary file lives in: its parent, or `.` for a bare name.
fn directory_of(destination: &Path) -> &Path {
    match destination.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

/// Creates a temporary file next to `destination`: `.NAME.lumen-PID-N.tmp`, with the first `N`
/// that is free.
fn create_temporary<F: Filesystem>(fs: &F, destination: &Path) -> io::Result<(F::File, PathBuf)> {
    let name = destination
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "has no file name"))?
        .to_string_lossy();
    let directory = directory_of(destination);
    let unique = fs.unique();
    for attempt in 0..NAME_TRIES {
        let path = directory.join(format!(".{name}.lumen-{unique}-{attempt}.tmp"));
        match fs.create_new(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "no free temporary file name"))
}

/// Refuses, before anything is rendered, a destination that publication would fail on: it
/// checks the destination's kind and permission, then creates and removes a temporary file in
/// its directory. Nothing is left behind.
///
/// # Errors
///
/// The reason the destination cannot be written.
pub(crate) fn preflight<F: Filesystem>(fs: &F, destination: &Path) -> Result<(), PublishError> {
    match fs.mode(destination).map_err(PublishError::Io)? {
        Mode::Replace => {
            let (file, path) = create_temporary(fs, destination).map_err(PublishError::Io)?;
            drop(file);
            fs.remove(&path).map_err(PublishError::Io)
        }
        Mode::InPlace => fs.open_in_place(destination).map(drop).map_err(PublishError::Io),
    }
}

/// Publishes `image` as a PNG at `destination` and returns the PNG's length.
///
/// # Errors
///
/// A [`PublishError`]; except for [`PublishError::DirectorySync`] the destination is as it was
/// and no temporary file is left (a temporary file that cannot be removed is named in the error).
pub(crate) fn publish<F: Filesystem>(
    fs: &F,
    destination: &Path,
    image: &Rgba8Image,
) -> Result<u64, PublishError> {
    if fs.mode(destination).map_err(PublishError::Io)? == Mode::InPlace {
        let file = fs.open_in_place(destination).map_err(PublishError::Io)?;
        // No sync: a device has no storage to force (`fsync` on /dev/null is `EINVAL`).
        flush_png(file, image)?;
        return Ok(png_len(image));
    }
    let (file, temporary) = create_temporary(fs, destination).map_err(PublishError::Io)?;
    let published = flush_png(file, image)
        .and_then(|mut file| fs.sync(&mut file).map_err(PublishError::Io))
        .and_then(|()| fs.rename(&temporary, destination).map_err(PublishError::Io));
    if let Err(error) = published {
        // Best effort: the original error is the one to report; a leftover is added to it.
        return Err(match fs.remove(&temporary) {
            Ok(()) => error,
            Err(left) => leftover(&error, &temporary, &left),
        });
    }
    fs.sync_directory(directory_of(destination)).map_err(PublishError::DirectorySync)?;
    Ok(png_len(image))
}

/// Writes the PNG to `file`, flushes the buffer and returns the file.
fn flush_png<W: Write>(file: W, image: &Rgba8Image) -> Result<W, PublishError> {
    let mut buffered = BufWriter::new(file);
    write_png(&mut buffered, image).map_err(PublishError::Png)?;
    buffered.into_inner().map_err(|error| PublishError::Png(PngError::Write(error.into_error())))
}

/// `error` with the note that the temporary file could not be removed either.
fn leftover(error: &PublishError, temporary: &Path, left: &io::Error) -> PublishError {
    PublishError::Io(io::Error::other(format!(
        "{error}; the temporary file {} could not be removed: {left}",
        temporary.display()
    )))
}

#[cfg(test)]
mod tests;
