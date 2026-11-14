//! The adapter to Bunny's bounding volume hierarchy: the tree [`Scene::new`] builds over the
//! triangles' boxes, and [`Tracer`] for [`Scene`] through Bunny's compact traversals.

use bunny_broadphase::{
    build_bvh_sah, compact_any_hit, compact_bvh, compact_closest_hit, BvhNode, CompactChild,
    CompactNode, CompactRoot, CompactView,
};
use bunny_geom::FixedAabb3;
use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use bunny_query::{TraceRay, TraceTriangle};

use crate::schedule::widen;

use super::{
    reserved, Hit, PrimitiveIndex, Scene, SceneBuffer, SceneError, TraceError, Tracer, FAR,
};

/// A compact BVH: its root, its nodes and the primitive-index buffer its leaves refer to.
#[derive(Clone, Debug)]
pub(super) struct Tree {
    root: Option<CompactRoot>,
    pub(super) nodes: Vec<CompactNode>,
    pub(super) prim_indices: Vec<u32>,
}

impl Scene {
    /// The tree as Bunny's compact traversals and the statistics walk read it.
    pub(super) fn view(&self) -> CompactView<'_> {
        CompactView {
            root: self.tree.root,
            nodes: &self.tree.nodes,
            prim_indices: &self.tree.prim_indices,
        }
    }

    /// The triangle Bunny's traversal names by its raw `u32` primitive index. `Scene::new` has
    /// checked that every index in the tree names a triangle, so for an index the traversal
    /// reports this is never `None`.
    fn triangle(&self, primitive: u32) -> Option<&TraceTriangle> {
        self.triangles.get(PrimitiveIndex::new(primitive).to_usize())
    }
}

impl Tracer for Scene {
    /// Bunny's `compact_closest_hit` over the scene's tree with `TraceRay::hit_triangle` as the
    /// primitive test (`t_min` zero, `t_max` the traversal's limit, at first [`FAR`]). The
    /// winner is tested once more, at exactly its distance, to read its full `TriangleHit`.
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let found = compact_closest_hit(self.view(), ray, FAR, |primitive, limit| {
            let triangle = self.triangle(primitive)?;
            ray.hit_triangle(triangle, FixedQ32_32::ZERO, limit).map(|hit| hit.distance)
        })
        .map_err(TraceError::Traversal)?;
        let Some(found) = found else { return Ok(None) };
        let primitive = PrimitiveIndex::new(found.primitive);
        let Some(triangle) = self.triangle(found.primitive) else {
            let triangles = self.triangles.len();
            return Err(TraceError::UnknownPrimitive { primitive, triangles });
        };
        let distance = found.distance;
        let hit = ray
            .hit_triangle(triangle, distance, distance)
            .ok_or(TraceError::HitNotRepeatable { primitive, distance })?;
        Ok(Some(Hit { primitive, hit }))
    }

    /// Bunny's `compact_any_hit` over the scene's tree with `TraceRay::hit_triangle` (`t_min`
    /// zero, `t_max` as given) as the primitive test.
    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        compact_any_hit(self.view(), ray, t_max, |primitive| {
            self.triangle(primitive).is_some_and(|triangle| {
                ray.hit_triangle(triangle, FixedQ32_32::ZERO, t_max).is_some()
            })
        })
        .map_err(TraceError::Traversal)
    }
}

/// A tree over `boxes` from `build_bvh_sah`, converted by `compact_bvh`: the compact root, the
/// compact nodes and the primitive-index buffer.
pub(super) fn build_tree(boxes: &[FixedAabb3]) -> Result<Tree, SceneError> {
    let count = boxes.len();
    let node_count =
        count.checked_mul(2).ok_or(SceneError::NodeCountOverflow { triangles: count })?;
    let empty_box = FixedAabb3::new(ZERO_VEC, ZERO_VEC);
    let empty = BvhNode { bounds: empty_box, first_child_or_prim_idx: 0, prim_count: 0 };
    let mut nodes = reserved(SceneBuffer::Nodes, node_count)?;
    nodes.resize(node_count, empty);
    let mut prim_indices = reserved(SceneBuffer::PrimitiveIndices, count)?;
    prim_indices.resize(count, 0_u32);
    let used = build_bvh_sah(&mut nodes, &mut prim_indices, boxes)
        .ok_or(SceneError::BvhBuild { triangles: count, nodes: node_count })?;
    // Checked once here, so the traversal callbacks below never meet an index past the
    // triangles: their `None` for a missing triangle is unreachable for a built scene.
    if let Some(&index) = prim_indices.iter().find(|&&index| widen(index) >= count) {
        return Err(SceneError::TreeNamesUnknownPrimitive { index, triangles: count });
    }
    nodes.truncate(used);
    // A tree of n nodes has (n - 1) / 2 interior nodes, one compact node each.
    let interior = used.saturating_sub(1) / 2;
    let blank = CompactNode {
        bounds: [empty_box, empty_box],
        children: [CompactChild::default(), CompactChild::default()],
    };
    let mut compact = reserved(SceneBuffer::CompactNodes, interior)?;
    compact.resize(interior, blank);
    let converted = compact_bvh(&nodes, &mut compact).map_err(SceneError::BvhCompact)?;
    let root = converted.map(|(root, written)| {
        compact.truncate(written);
        root
    });
    Ok(Tree { root, nodes: compact, prim_indices })
}

const ZERO_VEC: FixedVec3 = FixedVec3::new(FixedQ32_32::ZERO, FixedQ32_32::ZERO, FixedQ32_32::ZERO);
