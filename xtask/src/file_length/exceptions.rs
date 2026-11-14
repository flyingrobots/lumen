//! The files allowed over their limit, each until its issue splits it.
//!
//! Add an entry only with an open issue that splits the file, at the file's current count.
//! The gate fails when a listed file grows past its count or no longer needs the entry.

use super::Exception;

/// Every file allowed over its limit.
pub(crate) const EXCEPTIONS: &[Exception] = &[];
