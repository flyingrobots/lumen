//! The traversal lines of `render --stats`: per kind of ray, Bunny's traversal counts in all, per
//! ray on average and at most (ADR 0008).

use std::io::{self, Write};

use bunny_broadphase::TraversalCounts;
use lumen::{RayOutcomes, TraversalStatistics, TraversalWork};

use super::Ratio;

/// How a counter is read from Bunny's record.
type Counter = fn(&TraversalCounts) -> u64;

/// The counters, as their keys name them, and how each is read.
const COUNTERS: [(&str, Counter); 6] = [
    ("box-tests", |c| c.box_tests),
    ("box-entries", |c| c.box_entries),
    ("popped-skipped", |c| c.popped_skipped),
    ("interior-visits", |c| c.interior_visits),
    ("leaf-visits", |c| c.leaf_visits),
    ("primitive-tests", |c| c.primitive_tests),
];

/// What Bunny's traversals did (`render --stats`), after the ray lines: for camera, bounce and
/// shadow rays in turn, each counter's total, its mean per ray of that kind (two places, rounded
/// to nearest, ties up) and the most any one ray took, then the deepest stack of any ray. Every
/// value is a function of the scene, the size and the settings.
pub(crate) fn write_traversal(
    out: &mut impl Write,
    traversal: &TraversalStatistics,
    rays: &RayOutcomes,
) -> io::Result<()> {
    for (kind, work, count) in [
        ("camera", &traversal.camera, rays.camera()),
        ("bounce", &traversal.bounce, rays.bounce()),
        ("shadow", &traversal.shadow, rays.shadow()),
    ] {
        write_kind(out, kind, work, count)?;
    }
    Ok(())
}

fn write_kind(out: &mut impl Write, kind: &str, work: &TraversalWork, rays: u64) -> io::Result<()> {
    for (name, read) in COUNTERS {
        let total = read(&work.total);
        writeln!(out, "traversal.{kind}.{name} {total}")?;
        writeln!(
            out,
            "traversal.{kind}.{name}-mean {}",
            Ratio(u128::from(total), u128::from(rays))
        )?;
        writeln!(out, "traversal.{kind}.{name}-max {}", read(&work.most))?;
    }
    writeln!(out, "traversal.{kind}.peak-stack {}", work.total.peak_stack)
}
