//! Publication's tests, on the real encoder's bytes and the real file system in a scratch
//! directory per test; [`Faulty`] fails one step of it, never the encoder.

use std::cell::RefCell;
use std::path::PathBuf;

use lumen::{write_png, ImageSize};

use super::*;

/// A fresh directory for one test, removed when dropped; its name holds the process id and the
/// test's name.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("lumen-publish-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch directory");
        Self(dir)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    /// The names in the directory, sorted.
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.0)
            .expect("dir")
            .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// An image whose PNG is several buffer flushes long (64 by 64 pixels, 16 KiB of pixels) with
/// bytes that depend on `seed`.
fn image(seed: u8) -> Rgba8Image {
    let size = ImageSize::new(64, 64).expect("valid");
    let bytes = (0..size.byte_len())
        .map(|i| u8::try_from(i % 251).expect("below 251").wrapping_mul(seed | 1))
        .collect();
    Rgba8Image::from_bytes(size, bytes).expect("length matches")
}

fn png(image: &Rgba8Image) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_png(&mut bytes, image).expect("writes to memory");
    bytes
}

const OLD: &[u8] = b"the file that was there";

/// A file that refuses bytes after `left` of them, writing the ones before.
struct Limited {
    file: File,
    left: usize,
}

impl Write for Limited {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Err(io::Error::other("disk full"));
        }
        let n = self.file.write(&buf[..buf.len().min(self.left)])?;
        self.left -= n;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// A step of publication that [`Faulty`] can fail.
#[derive(Clone, Copy, PartialEq)]
enum Step {
    Sync,
    Rename,
    Remove,
    DirectorySync,
}

/// The real file system with one step made to fail, and an optional action run at the first
/// `sync` (while the temporary file exists and the rename has not happened).
#[derive(Default)]
struct Faulty<'a> {
    write_limit: Option<usize>,
    /// The steps that fail.
    fails: &'static [Step],
    at_first_sync: RefCell<Option<Box<dyn FnOnce() + 'a>>>,
}

impl Faulty<'_> {
    fn fails(&self, step: Step) -> bool {
        self.fails.contains(&step)
    }
}

impl Filesystem for Faulty<'_> {
    type File = Limited;

    fn unique(&self) -> String {
        Disk.unique()
    }

    fn mode(&self, destination: &Path) -> io::Result<Mode> {
        Disk.mode(destination)
    }

    fn create_new(&self, path: &Path) -> io::Result<Limited> {
        Ok(Limited { file: Disk.create_new(path)?, left: self.write_limit.unwrap_or(usize::MAX) })
    }

    fn open_in_place(&self, path: &Path) -> io::Result<Limited> {
        Ok(Limited { file: Disk.open_in_place(path)?, left: usize::MAX })
    }

    fn sync(&self, file: &mut Limited) -> io::Result<()> {
        if let Some(action) = self.at_first_sync.borrow_mut().take() {
            action();
        }
        if self.fails(Step::Sync) {
            return Err(io::Error::other("sync refused"));
        }
        Disk.sync(&mut file.file)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.fails(Step::Rename) {
            return Err(io::Error::other("rename refused"));
        }
        Disk.rename(from, to)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        if self.fails(Step::Remove) {
            return Err(io::Error::other("remove refused"));
        }
        Disk.remove(path)
    }

    fn sync_directory(&self, directory: &Path) -> io::Result<()> {
        if self.fails(Step::DirectorySync) {
            return Err(io::Error::other("directory sync refused"));
        }
        Disk.sync_directory(directory)
    }
}

#[test]
fn a_successful_publication_leaves_exactly_the_png_and_nothing_else() {
    let dir = Scratch::new("success");
    let dest = dir.file("a.png");
    let image = image(3);
    for existing in [false, true] {
        if existing {
            fs::write(&dest, OLD).expect("old file");
        }
        assert_eq!(publish(&Disk, &dest, &image).expect("publishes"), png_len(&image));
        assert_eq!(fs::read(&dest).expect("file"), png(&image), "existing: {existing}");
        assert_eq!(dir.names(), ["a.png"]);
    }
}

/// The mutation oracle: writing straight to the destination truncates the old file before the
/// failure, which this catches.
#[test]
fn a_write_that_fails_part_way_leaves_the_old_file_and_no_temporary_file() {
    let dir = Scratch::new("part-way");
    let dest = dir.file("keep.png");
    for limit in [0, 100, 9_000] {
        fs::write(&dest, OLD).expect("old file");
        let faulty = Faulty { write_limit: Some(limit), ..Faulty::default() };
        let error = publish(&faulty, &dest, &image(3)).expect_err("fails");
        assert!(error.to_string().contains("disk full"), "{error}");
        assert_eq!(fs::read(&dest).expect("file"), OLD, "limit {limit}");
        assert_eq!(dir.names(), ["keep.png"], "limit {limit}");
    }
}

#[test]
fn a_failed_sync_or_rename_leaves_the_old_file_and_no_temporary_file() {
    let dir = Scratch::new("sync-rename");
    let dest = dir.file("keep.png");
    let cases = [
        (Faulty { fails: &[Step::Sync], ..Faulty::default() }, "sync refused"),
        (Faulty { fails: &[Step::Rename], ..Faulty::default() }, "rename refused"),
    ];
    for (faulty, message) in cases {
        fs::write(&dest, OLD).expect("old file");
        let error = publish(&faulty, &dest, &image(3)).expect_err("fails");
        assert!(error.to_string().contains(message), "{error}");
        assert_eq!(fs::read(&dest).expect("file"), OLD, "{message}");
        assert_eq!(dir.names(), ["keep.png"], "{message}");
    }
}

#[test]
fn a_temporary_file_that_cannot_be_removed_is_named_in_the_error() {
    let dir = Scratch::new("leftover");
    let dest = dir.file("keep.png");
    fs::write(&dest, OLD).expect("old file");
    let faulty = Faulty { write_limit: Some(10), fails: &[Step::Remove], ..Faulty::default() };
    let error = publish(&faulty, &dest, &image(3)).expect_err("fails").to_string();
    assert!(error.contains("disk full"), "{error}");
    assert!(error.contains("lumen-") && error.contains("could not be removed"), "{error}");
    assert_eq!(fs::read(&dest).expect("file"), OLD);
}

#[test]
fn a_failed_directory_sync_is_reported_after_the_file_was_replaced() {
    let dir = Scratch::new("directory-sync");
    let dest = dir.file("a.png");
    fs::write(&dest, OLD).expect("old file");
    let faulty = Faulty { fails: &[Step::DirectorySync], ..Faulty::default() };
    let error = publish(&faulty, &dest, &image(3)).expect_err("reported");
    assert!(matches!(error, PublishError::DirectorySync(_)), "{error}");
    assert_eq!(fs::read(&dest).expect("file"), png(&image(3)));
    assert_eq!(dir.names(), ["a.png"]);
}

/// Two publications to one destination, interleaved at the point where they are most exposed:
/// the second runs completely while the first holds its temporary file. Each leaves a complete
/// image, the second's names its own temporary file, and the first (renamed last) wins.
#[test]
fn two_publications_to_one_destination_each_leave_a_complete_file() {
    let dir = Scratch::new("two");
    let dest = dir.file("a.png");
    let (first, second) = (image(3), image(5));
    let inner = || {
        publish(&Disk, &dest, &second).expect("the second publishes");
        assert_eq!(fs::read(&dest).expect("file"), png(&second));
        assert_eq!(dir.names().len(), 2, "the first's temporary file and the destination");
    };
    let faulty = Faulty { at_first_sync: RefCell::new(Some(Box::new(inner))), ..Faulty::default() };
    publish(&faulty, &dest, &first).expect("the first publishes");
    assert_eq!(fs::read(&dest).expect("file"), png(&first));
    assert_eq!(dir.names(), ["a.png"]);
}

#[test]
fn a_file_at_a_temporary_name_is_never_clobbered() {
    let dir = Scratch::new("collision");
    let dest = dir.file("a.png");
    let taken = dir.file(&format!(".a.png.lumen-{}-0.tmp", Disk.unique()));
    fs::write(&taken, OLD).expect("somebody's file");
    publish(&Disk, &dest, &image(3)).expect("publishes under another name");
    assert_eq!(fs::read(&taken).expect("file"), OLD);
    assert_eq!(fs::read(&dest).expect("file"), png(&image(3)));
    assert_eq!(dir.names().len(), 2);
}

#[test]
fn preflight_refuses_what_publication_could_not_write_and_leaves_nothing() {
    let dir = Scratch::new("preflight");
    let readonly = dir.file("readonly.png");
    fs::write(&readonly, OLD).expect("old file");
    let mut permissions = fs::metadata(&readonly).expect("meta").permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&readonly, permissions).expect("read-only");
    let subdirectory = dir.file("sub");
    fs::create_dir(&subdirectory).expect("sub");
    for (destination, message) in [
        (dir.file("missing").join("a.png"), ""),
        (subdirectory.clone(), "is a directory"),
        (readonly.clone(), "read-only"),
    ] {
        let error = preflight(&Disk, &destination).expect_err("refused").to_string();
        assert!(error.contains(message), "{destination:?}: {error}");
        let error = publish(&Disk, &destination, &image(3)).expect_err("refused").to_string();
        assert!(error.contains(message), "{destination:?}: {error}");
    }
    assert_eq!(fs::read(&readonly).expect("file"), OLD);
    assert_eq!(dir.names(), ["readonly.png", "sub"]);
    preflight(&Disk, &dir.file("fine.png")).expect("a free name is fine");
    assert_eq!(dir.names(), ["readonly.png", "sub"]);
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::symlink;

    use super::*;

    /// A link is replaced by the file; what it pointed at is not written.
    #[test]
    fn a_symlink_is_replaced_and_its_target_is_untouched() {
        let dir = Scratch::new("symlink");
        let (target, link) = (dir.file("target.png"), dir.file("link.png"));
        fs::write(&target, OLD).expect("target");
        symlink("target.png", &link).expect("link");
        publish(&Disk, &link, &image(3)).expect("publishes");
        assert_eq!(fs::read(&target).expect("target"), OLD);
        assert_eq!(fs::read(&link).expect("file"), png(&image(3)));
        assert!(!fs::symlink_metadata(&link).expect("meta").is_symlink());
        assert_eq!(dir.names(), ["link.png", "target.png"]);
    }

    #[test]
    fn a_dangling_symlink_is_replaced_and_a_link_to_a_directory_is_refused() {
        let dir = Scratch::new("symlink-kinds");
        let (dangling, to_directory) = (dir.file("dangling.png"), dir.file("to-dir"));
        symlink("nowhere", &dangling).expect("link");
        symlink(&dir.0, &to_directory).expect("link");
        publish(&Disk, &dangling, &image(3)).expect("publishes");
        assert_eq!(fs::read(&dangling).expect("file"), png(&image(3)));
        let error = publish(&Disk, &to_directory, &image(3)).expect_err("refused");
        assert!(error.to_string().contains("is a directory"), "{error}");
    }

    /// A device is written in place, so `--out /dev/null` keeps working.
    #[test]
    fn a_device_is_written_in_place() {
        let null = Path::new("/dev/null");
        assert_eq!(Disk.mode(null).expect("mode"), Mode::InPlace);
        preflight(&Disk, null).expect("preflight");
        assert_eq!(publish(&Disk, null, &image(3)).expect("publishes"), png_len(&image(3)));
    }

    /// The new file has the default mode for a new file, not the old file's mode.
    #[test]
    fn the_old_files_mode_is_not_carried_over() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new("mode");
        let dest = dir.file("a.png");
        let probe = dir.file("probe");
        fs::write(&probe, OLD).expect("probe");
        let default_mode = fs::metadata(&probe).expect("meta").permissions().mode();
        fs::write(&dest, OLD).expect("old");
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o640)).expect("mode");
        publish(&Disk, &dest, &image(3)).expect("publishes");
        assert_eq!(fs::metadata(&dest).expect("meta").permissions().mode(), default_mode);
    }
}

/// A path with no final component has no name for a temporary file (a path ending in `..` is
/// such a path on Unix; Windows resolves it lexically to a directory, so the empty path is used).
#[test]
fn a_path_with_no_file_name_is_refused() {
    let error = create_temporary(&Disk, Path::new("")).map(|(_, path)| path).expect_err("refused");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("no file name"), "{error}");
}

#[test]
fn a_bare_file_name_lives_in_the_current_directory() {
    assert_eq!(directory_of(Path::new("a.png")), Path::new("."));
    assert_eq!(directory_of(Path::new("d/a.png")), Path::new("d"));
    assert_eq!(directory_of(Path::new("/a.png")), Path::new("/"));
}

/// A writer that takes at most a few bytes per call.
struct Short(Vec<u8>);

impl Write for Short {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = buf.len().min(5);
        self.0.extend_from_slice(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn the_count_is_what_the_writer_accepted_across_short_writes() {
    let image = image(3);
    let (short, bytes) = flush_png(Short(Vec::new()), &image, png_len(&image)).expect("writes");
    assert_eq!(bytes, png_len(&image));
    assert_eq!(u64::try_from(short.0.len()), Ok(bytes));
}

/// A count that differs from the expected length is an error, and nothing is published.
#[test]
fn a_count_that_differs_from_the_expected_length_publishes_nothing() {
    let dir = Scratch::new("count");
    let dest = dir.file("keep.png");
    fs::write(&dest, OLD).expect("old file");
    let image = image(3);
    let expected = png_len(&image) + 1;
    let error = publish_expecting(&Disk, &dest, &image, expected).expect_err("differs");
    assert_eq!(
        error.to_string(),
        format!("wrote {} bytes where the PNG is {expected}", expected - 1)
    );
    assert_eq!(fs::read(&dest).expect("file"), OLD);
    assert_eq!(dir.names(), ["keep.png"]);
}

/// The count `publish` returns is the length of the file it renamed into place.
#[test]
fn the_returned_count_is_the_length_of_the_published_file() {
    let dir = Scratch::new("returned-count");
    let dest = dir.file("a.png");
    let bytes = publish(&Disk, &dest, &image(3)).expect("publishes");
    assert_eq!(bytes, fs::metadata(&dest).expect("meta").len());
}

/// The longest file name ext4, APFS and (in UTF-16 units) NTFS accept: 255.
const NAME_MAX: usize = 255;

/// The real file system with a limit of [`NAME_MAX`] bytes on the name of every file it creates
/// or renames, checked before the call, whatever the host's own limit: a longer name fails as
/// Linux's `ENAMETOOLONG` does.
struct NameLimit;

impl NameLimit {
    fn check(path: &Path) -> io::Result<()> {
        let length = path.file_name().map_or(0, std::ffi::OsStr::len);
        if length > NAME_MAX {
            return Err(io::Error::new(io::ErrorKind::InvalidFilename, "File name too long"));
        }
        Ok(())
    }
}

impl Filesystem for NameLimit {
    type File = File;

    fn unique(&self) -> String {
        Disk.unique()
    }

    fn mode(&self, destination: &Path) -> io::Result<Mode> {
        Disk.mode(destination)
    }

    fn create_new(&self, path: &Path) -> io::Result<File> {
        Self::check(path)?;
        Disk.create_new(path)
    }

    fn open_in_place(&self, path: &Path) -> io::Result<File> {
        Disk.open_in_place(path)
    }

    fn sync(&self, file: &mut File) -> io::Result<()> {
        Disk.sync(file)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        Self::check(from)?;
        Self::check(to)?;
        Disk.rename(from, to)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        Disk.remove(path)
    }

    fn sync_directory(&self, directory: &Path) -> io::Result<()> {
        Disk.sync_directory(directory)
    }
}

/// A file name of `bytes` bytes ending `.png`: `a` repeated.
fn long_name(bytes: usize) -> String {
    format!("{}.png", "a".repeat(bytes - 4))
}

/// Oracle: the file system's limit. A destination whose name is 244 bytes (the review's case,
/// refused before) or exactly 255 is checked and published through the name limit and through
/// the host's own file system, leaving exactly the image. Witness: creating a file of that name
/// directly works.
#[test]
fn a_destination_name_up_to_the_file_systems_limit_is_published() {
    let dir = Scratch::new("long-name");
    for bytes in [244, NAME_MAX] {
        let name = long_name(bytes);
        let dest = dir.file(&name);
        fs::write(&dest, OLD).expect("a name of this length can be created directly");
        fs::remove_file(&dest).expect("removed");
        preflight(&NameLimit, &dest).expect("the limit admits it");
        assert_eq!(publish(&NameLimit, &dest, &image(3)).ok(), Some(png_len(&image(3))));
        assert_eq!(fs::read(&dest).expect("file"), png(&image(3)), "{bytes} bytes");
        assert_eq!(dir.names(), std::slice::from_ref(&name));
        preflight(&Disk, &dest).expect("the host admits it");
        publish(&Disk, &dest, &image(5)).expect("the host publishes it");
        assert_eq!(fs::read(&dest).expect("file"), png(&image(5)), "{bytes} bytes");
        assert_eq!(dir.names(), [name]);
        fs::remove_file(&dest).expect("removed");
    }
}

/// Oracle: the atomic guarantees again, at the longest name: a failed rename leaves the old file
/// and no temporary file.
#[test]
fn a_failure_at_the_longest_name_leaves_the_old_file_and_no_temporary_file() {
    let dir = Scratch::new("long-name-failure");
    let name = long_name(NAME_MAX);
    let dest = dir.file(&name);
    fs::write(&dest, OLD).expect("old file");
    let faulty = Faulty { fails: &[Step::Rename], ..Faulty::default() };
    let error = publish(&faulty, &dest, &image(3)).expect_err("fails").to_string();
    assert!(error.contains("rename refused"), "{error}");
    assert_eq!(fs::read(&dest).expect("file"), OLD);
    assert_eq!(dir.names(), [name]);
}

/// Oracle: the temporary name's stated form, `.PREFIX.lumen-PID-N.tmp`, with `PREFIX` the
/// destination's name cut to at most 32 bytes at a character boundary, seen while it exists (at
/// the first sync). A 247-byte name of `a` and 121 two-byte `é`s keeps `a` and 15 of them (31
/// bytes: a 16th would end at byte 33); 244 bytes of `a` keep exactly 32; a short name is kept
/// whole.
#[test]
fn the_temporary_name_keeps_a_bounded_prefix_of_the_destinations() {
    let dir = Scratch::new("temporary-name");
    let unique = Disk.unique();
    for (name, prefix) in [
        (format!("a{}.png", "é".repeat(121)), format!("a{}", "é".repeat(15))),
        (long_name(244), "a".repeat(32)),
        ("a.png".to_owned(), "a.png".to_owned()),
    ] {
        let seen = RefCell::new(Vec::new());
        let record = || *seen.borrow_mut() = dir.names();
        let faulty =
            Faulty { at_first_sync: RefCell::new(Some(Box::new(record))), ..Faulty::default() };
        publish(&faulty, &dir.file(&name), &image(3)).expect("publishes");
        let want = format!(".{prefix}.lumen-{unique}-0.tmp");
        assert!(want.len() <= 1 + 32 + 24, "{want}");
        assert_eq!(*seen.borrow(), [want]);
        assert_eq!(dir.names(), std::slice::from_ref(&name));
        fs::remove_file(dir.file(&name)).expect("removed");
    }
}
