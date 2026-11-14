//! What each worker of a render did: [`RunStatistics`], facts about the run, not the image.

use super::{RayCount, RayStatistics};
use crate::workers::WorkerRun;

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

impl From<&WorkerRun<RayStatistics>> for WorkerStatistics {
    fn from(run: &WorkerRun<RayStatistics>) -> Self {
        Self { bands: run.bands, rows: run.rows, pixels: run.total.pixels, rays: run.total.count() }
    }
}
