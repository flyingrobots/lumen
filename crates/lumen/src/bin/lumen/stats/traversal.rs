//! The `traversal_statistics` message of `render --stats`: per kind of ray, Bunny's traversal
//! counts in all and at most (ADR 0008).

use bunny_broadphase::TraversalCounts;
use lumen::{TraversalStatistics, TraversalWork};

use crate::json::Object;
use crate::messages::message;

/// How a counter is read from Bunny's record.
type Counter = fn(&TraversalCounts) -> u64;

/// The counters, as their members name them, the most any one ray took as the same name with
/// `_max`, and how each is read.
const COUNTERS: [(&str, &str, Counter); 6] = [
    ("box_tests", "box_tests_max", |c| c.box_tests),
    ("box_entries", "box_entries_max", |c| c.box_entries),
    ("popped_skipped", "popped_skipped_max", |c| c.popped_skipped),
    ("interior_visits", "interior_visits_max", |c| c.interior_visits),
    ("leaf_visits", "leaf_visits_max", |c| c.leaf_visits),
    ("primitive_tests", "primitive_tests_max", |c| c.primitive_tests),
];

/// What Bunny's traversals did: for camera, bounce and shadow rays, each counter's total, then
/// the most any one ray took of each, then the deepest stack of any ray. The mean per ray is
/// the total over the kind's ray count in `ray_statistics`. Every value is a function of the
/// scene, the size and the settings.
pub(crate) fn traversal_statistics(traversal: &TraversalStatistics) -> Object {
    message("traversal_statistics")
        .with("camera", kind(&traversal.camera))
        .with("bounce", kind(&traversal.bounce))
        .with("shadow", kind(&traversal.shadow))
}

fn kind(work: &TraversalWork) -> Object {
    let totals =
        COUNTERS.iter().fold(Object::new(), |o, (name, _, read)| o.with(*name, read(&work.total)));
    let most = COUNTERS.iter().fold(totals, |o, (_, max, read)| o.with(*max, read(&work.most)));
    most.with("peak_stack", work.total.peak_stack)
}
