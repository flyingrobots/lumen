//! The shape of a scene's tree: [`TreeStatistics`], from one walk of Bunny's compact nodes.

use bunny_broadphase::bvh::SAH_MAX_LEAF_SIZE;
use bunny_broadphase::{CompactChild, CompactView};
use bunny_geom::FixedAabb3;

use super::StatisticsError;
use crate::schedule::widen;

/// The number of entries of [`TreeStatistics::leaf_sizes`]: leaves of 1, 2, 3 and 4 triangles
/// (Bunny's `SAH_MAX_LEAF_SIZE`, the largest leaf its builder keeps by choice), and leaves of
/// more, which the builder makes only at its depth cap (`SAH_MAX_DEPTH`, 60).
pub const LEAF_SIZE_BUCKETS: usize = SAH_MAX_LEAF_SIZE + 1;

/// The deepest the walk goes, and so its stack: the depth Bunny's `compact_bvh` accepts.
const WALK_DEPTH: usize = 64;

/// The shape of a scene's tree, in the terms of Bunny's `BvhNode` tree before conversion: every
/// node, interior or leaf, counts once. The compact layout stores only the interior nodes;
/// a leaf lives in its parent's child slot.
///
/// Depths count edges from the root, which is at depth 0 (a one-triangle scene's tree is one
/// leaf at depth 0). The mean leaf depth is `leaf_depth_total / leaves`; lumen gives both
/// integers and leaves the division to the caller, so no rounding happens here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TreeStatistics {
    /// Every node, interior and leaf: `interior_nodes + leaves`. A binary tree has one more leaf
    /// than interior nodes, so this is `2 * leaves - 1`, and at most `2 * triangles - 1`.
    pub nodes: usize,
    /// The interior nodes: Bunny's compact nodes.
    pub interior_nodes: usize,
    /// The leaves.
    pub leaves: usize,
    /// The depth of the deepest leaf. At most Bunny's `SAH_MAX_DEPTH` (60).
    pub max_leaf_depth: u32,
    /// The sum of every leaf's depth.
    pub leaf_depth_total: u64,
    /// The triangles of every leaf, summed: each triangle is in exactly one leaf, so this is the
    /// scene's triangle count.
    pub leaf_triangles: u64,
    /// The fewest triangles in a leaf.
    pub min_leaf_triangles: u32,
    /// The most triangles in a leaf.
    pub max_leaf_triangles: u32,
    /// How many leaves hold 1, 2, 3 and 4 triangles, and, last, how many hold more.
    pub leaf_sizes: [usize; LEAF_SIZE_BUCKETS],
}

impl TreeStatistics {
    const EMPTY: Self = Self {
        nodes: 0,
        interior_nodes: 0,
        leaves: 0,
        max_leaf_depth: 0,
        leaf_depth_total: 0,
        leaf_triangles: 0,
        min_leaf_triangles: u32::MAX,
        max_leaf_triangles: 0,
        leaf_sizes: [0; LEAF_SIZE_BUCKETS],
    };

    fn interior(&mut self) {
        self.interior_nodes = self.interior_nodes.saturating_add(1);
        self.nodes = self.nodes.saturating_add(1);
    }

    fn leaf(&mut self, triangles: u32, depth: u32) {
        self.leaves = self.leaves.saturating_add(1);
        self.nodes = self.nodes.saturating_add(1);
        self.max_leaf_depth = self.max_leaf_depth.max(depth);
        self.leaf_depth_total = self.leaf_depth_total.saturating_add(u64::from(depth));
        self.leaf_triangles = self.leaf_triangles.saturating_add(u64::from(triangles));
        self.min_leaf_triangles = self.min_leaf_triangles.min(triangles);
        self.max_leaf_triangles = self.max_leaf_triangles.max(triangles);
        // A leaf holds at least one triangle; `saturating_sub` keeps a malformed zero in the
        // first bucket rather than wrapping.
        let bucket = widen(triangles.saturating_sub(1)).min(LEAF_SIZE_BUCKETS - 1);
        if let Some(count) = self.leaf_sizes.get_mut(bucket) {
            *count = count.saturating_add(1);
        }
    }
}

/// The root box and the shape of the tree `view`: a depth-first walk from the root, left child
/// first, with a stack of [`WALK_DEPTH`] entries.
///
/// The sums cannot saturate: a tree has fewer than `2 * u32::MAX` nodes, each leaf at most 64
/// deep.
pub(super) fn tree_statistics(
    view: CompactView<'_>,
) -> Result<(FixedAabb3, TreeStatistics), StatisticsError> {
    let malformed = |node: u32| StatisticsError::MalformedTree { node };
    let root = view.root.ok_or(malformed(0))?;
    let mut shape = TreeStatistics::EMPTY;
    let mut stack: [(CompactChild, u32); WALK_DEPTH] = [(root.child, 0); WALK_DEPTH];
    let mut len = 1_usize;
    let mut visits = 0_usize;
    while let Some(top) = len.checked_sub(1) {
        len = top;
        let &(child, depth) = stack.get(top).ok_or(malformed(0))?;
        if child.is_leaf() {
            shape.leaf(child.prim_count, depth);
            continue;
        }
        visits = visits.saturating_add(1);
        let node = view.nodes.get(widen(child.first)).filter(|_| visits <= view.nodes.len());
        let node = node.ok_or(malformed(child.first))?;
        shape.interior();
        // Right underneath, so the left subtree is walked first.
        for &next in node.children.iter().rev() {
            *stack.get_mut(len).ok_or(malformed(child.first))? = (next, depth.saturating_add(1));
            len = len.saturating_add(1);
        }
    }
    Ok((root.bounds, shape))
}
