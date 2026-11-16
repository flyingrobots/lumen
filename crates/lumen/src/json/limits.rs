//! The reader's limits: [`JsonLimits`].

use core::fmt;

/// The default depth limit: 128 open containers, far past any job or scene file and well inside
/// [`JSON_MAX_DEPTH_CEILING`].
pub const JSON_DEFAULT_MAX_DEPTH: usize = 128;

/// The default size limit: 16 MiB (`2^24` bytes) of input. A scene file with its meshes inline
/// would need more; its reader passes its own limit.
pub const JSON_DEFAULT_MAX_BYTES: usize = 1 << 24;

/// The largest depth limit [`JsonLimits::new`] accepts. The reader itself does not recurse, but
/// the tree's `Drop`, `Clone`, `Debug` and `PartialEq` do, one stack frame per level, so the
/// limit keeps them inside a test thread's 2 MiB stack.
pub const JSON_MAX_DEPTH_CEILING: usize = 1024;

/// The limits of one read: how deep containers may nest and how long the input may be.
///
/// # Examples
///
/// ```
/// use lumen::{parse_json, JsonLimits};
///
/// let limits = JsonLimits::new(2, 64)?;
/// assert!(parse_json(b"[[]]", limits).is_ok());
/// assert!(parse_json(b"[[[]]]", limits).is_err());
/// # Ok::<(), lumen::JsonLimitError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonLimits {
    max_depth: usize,
    max_bytes: usize,
}

impl JsonLimits {
    /// Limits of `max_depth` open containers (0 admits scalars only) and `max_bytes` bytes of
    /// input (the input may be exactly that long).
    ///
    /// # Errors
    ///
    /// [`JsonLimitError`] if `max_depth` is above [`JSON_MAX_DEPTH_CEILING`].
    pub const fn new(max_depth: usize, max_bytes: usize) -> Result<Self, JsonLimitError> {
        if max_depth > JSON_MAX_DEPTH_CEILING {
            return Err(JsonLimitError { max_depth });
        }
        Ok(Self { max_depth, max_bytes })
    }

    /// The deepest nesting of containers admitted.
    #[must_use]
    pub const fn max_depth(self) -> usize {
        self.max_depth
    }

    /// The longest input admitted, in bytes.
    #[must_use]
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }
}

impl Default for JsonLimits {
    /// [`JSON_DEFAULT_MAX_DEPTH`] and [`JSON_DEFAULT_MAX_BYTES`].
    fn default() -> Self {
        Self { max_depth: JSON_DEFAULT_MAX_DEPTH, max_bytes: JSON_DEFAULT_MAX_BYTES }
    }
}

/// A depth limit above [`JSON_MAX_DEPTH_CEILING`] was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonLimitError {
    max_depth: usize,
}

impl JsonLimitError {
    /// The depth limit that was refused.
    #[must_use]
    pub const fn max_depth(self) -> usize {
        self.max_depth
    }
}

impl fmt::Display for JsonLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a JSON depth limit of {} is above the ceiling of {JSON_MAX_DEPTH_CEILING}",
            self.max_depth
        )
    }
}

impl std::error::Error for JsonLimitError {}
