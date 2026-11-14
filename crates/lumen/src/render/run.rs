//! What each worker of a render did: [`RunStatistics`], facts about the run, not the image.

use super::{RayCount, RayStatistics};

/// How a render's work was spread over its workers (ADR 0008).
///
/// The band height, the band count and the worker count follow from the size and the schedule.
/// Which worker took which band does not: it depends on timing, so the split differs from run to
/// run and is information only. The workers' figures add up to the render's totals.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RunStatistics {
    /// The rows in a band.
    pub band_rows: u32,
    /// The bands the image was cut into.
    pub bands: u32,
    /// Each worker, worker 0 (the calling thread) first.
    pub workers: Vec<WorkerStatistics>,
}

/// What one worker did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkerStatistics {
    /// The bands it took.
    pub bands: u32,
    /// The rows it rendered.
    pub rows: u32,
    /// The pixels it rendered.
    pub pixels: u64,
    /// The rays its pixels traced.
    pub rays: RayCount,
}

impl WorkerStatistics {
    /// One worker's record in an image `width` pixels wide. In a render that succeeded every
    /// worker rendered whole rows, so its rows are its pixels over the width: counted per pixel
    /// already, not again per row.
    pub(super) fn of(bands: u32, total: &RayStatistics, width: u32) -> Self {
        let pixels = total.pixels;
        // At most the image's height, a `u32`; the width is at least 1.
        let rows = pixels.checked_div(u64::from(width)).unwrap_or(0);
        let rows = u32::try_from(rows).unwrap_or(u32::MAX);
        Self { bands, rows, pixels, rays: total.count() }
    }
}
