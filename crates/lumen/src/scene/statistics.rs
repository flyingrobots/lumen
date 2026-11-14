//! What a built scene holds: [`SceneStatistics`], read from a [`Scene`] on request.
//!
//! This module owns counting a built scene's contents: its triangles in all and by material, the
//! shape of its tree ([`TreeStatistics`]) and the bytes its buffers hold ([`SceneMemory`]). The
//! counts are read from the scene as [`Scene::new`] built it, after the fact; nothing here runs
//! while a ray is traced, and nothing in the scene changes (ADR 0008).

use core::fmt;
use core::mem::size_of_val;
use std::collections::TryReserveError;

use bunny_geom::FixedAabb3;

use super::Scene;
use crate::schedule::widen;

mod walk;

pub use walk::{TreeStatistics, LEAF_SIZE_BUCKETS};

/// The tree builder every scene uses: Bunny's binned surface-area-heuristic builder, at the
/// Bunny commit [`BUNNY_REV`](crate::BUNNY_REV). Bunny names its builders by function, not by a
/// versioned identity, so this is the function's path.
pub const TREE_BUILDER: &str = "bunny_broadphase::build_bvh_sah";

/// The tree layout every scene is traced in: Bunny's compact layout, in which an interior node
/// holds both children's boxes, searched by `compact_closest_hit` and `compact_any_hit`.
pub const TREE_LAYOUT: &str = "bunny_broadphase::compact_bvh";

/// The counts of a built scene: its triangles, its tree and its memory.
///
/// Every value is a function of the [`SceneSpec`](super::SceneSpec) the scene was built from
/// (the tree is a deterministic function of the triangles' boxes) except the byte counts of
/// [`SceneMemory`], which are this build's type sizes times those counts. None depends on a
/// render, a thread or a clock.
///
/// Non-exhaustive: a later version may count more, so it is built only by
/// [`Scene::statistics`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SceneStatistics {
    /// Every triangle, zero-area ones included.
    pub triangles: usize,
    /// The triangles that name each material, indexed by material in table order: one entry per
    /// albedo, zero for an albedo no triangle names. The entries sum to `triangles`.
    pub triangles_by_material: Vec<usize>,
    /// The triangles with at least one vertex normal, which the smooth integrator shades smooth
    /// ([`Scene::smooth_triangles`]).
    pub smooth_triangles: usize,
    /// The triangles of zero area. They are kept, in the tree and the content identity, but have
    /// no surface, and Bunny's exact triangle test never reports a hit on them.
    pub zero_area_triangles: usize,
    /// The box around every triangle: the root box of the tree.
    pub bounds: FixedAabb3,
    /// The shape of the tree.
    pub tree: TreeStatistics,
    /// The bytes of the scene's buffers.
    pub memory: SceneMemory,
}

/// The bytes of a scene's buffers: each buffer's length times the size of its element in this
/// build, `size_of_val` of the slice.
///
/// Not counted: the `Scene` value itself and its vectors' headers, spare capacity, the
/// allocator's overhead, the buffers `Scene::new` drops after the build (the triangles' boxes
/// and Bunny's `BvhNode` tree before conversion, `2 * triangles` nodes) and anything a render
/// allocates (the image, per-thread stacks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SceneMemory {
    /// Bunny's `TraceTriangle`s, one per triangle.
    pub triangles: usize,
    /// Bunny's `TriangleSurface`s, one `Option` per triangle (`None` for zero area).
    pub surfaces: usize,
    /// The vertex normals: one entry of three per triangle, or none.
    pub vertex_normals: usize,
    /// The material index of each triangle.
    pub material_indices: usize,
    /// The albedos.
    pub materials: usize,
    /// The tree's compact nodes, one per interior node.
    pub tree_nodes: usize,
    /// The tree's primitive-index buffer, one entry per triangle.
    pub tree_primitive_indices: usize,
}

impl SceneMemory {
    /// The sum of every buffer. Each is an allocation that exists, so the sum fits a `usize` and
    /// never saturates.
    #[must_use]
    pub fn total(&self) -> usize {
        [
            self.triangles,
            self.surfaces,
            self.vertex_normals,
            self.material_indices,
            self.materials,
            self.tree_nodes,
            self.tree_primitive_indices,
        ]
        .into_iter()
        .fold(0, usize::saturating_add)
    }
}

/// Why a scene's statistics could not be read.
///
/// Non-exhaustive: a later version may count something that can fail in a new way. Not `Copy`:
/// it can hold the allocator's error.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StatisticsError {
    /// The per-material counts could not be reserved.
    OutOfMemory {
        /// The number of materials, the length asked for.
        materials: usize,
        /// The allocator's error.
        error: TryReserveError,
    },
    /// The walk of the tree met a compact node that is not where Bunny's `compact_bvh` puts one:
    /// missing (the root included), outside the node list, deeper than 64 levels, or visited
    /// more often than there are nodes. `compact_bvh` refuses such a tree when the scene is
    /// built, so a built scene never reports this.
    MalformedTree {
        /// The compact node index the walk was at.
        node: u32,
    },
}

impl fmt::Display for StatisticsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfMemory { materials, error } => {
                write!(f, "out of memory counting the triangles of {materials} materials: {error}")
            }
            Self::MalformedTree { node } => {
                write!(f, "the scene's tree is malformed at compact node {node}")
            }
        }
    }
}

impl std::error::Error for StatisticsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::OutOfMemory { error, .. } => Some(error),
            Self::MalformedTree { .. } => None,
        }
    }
}

impl Scene {
    /// The scene's statistics: its triangles in all and by material, its smooth and zero-area
    /// triangles, its bounds, the shape of its tree and the bytes of its buffers.
    ///
    /// Computed on each call, in one pass over the material indices and one walk of the tree:
    /// nothing is collected when the scene is built or traced. The walk reads Bunny's public
    /// compact nodes; it is not a traversal and calls no Bunny arithmetic.
    ///
    /// # Errors
    ///
    /// [`StatisticsError::OutOfMemory`] if the per-material counts cannot be reserved, and
    /// [`StatisticsError::MalformedTree`], which a built scene never reports.
    ///
    /// # Examples
    ///
    /// ```
    /// let scene = lumen::reference_scene()?;
    /// let stats = scene.statistics()?;
    /// assert_eq!(stats.triangles, scene.triangles().len());
    /// assert_eq!(stats.triangles_by_material.iter().sum::<usize>(), stats.triangles);
    /// assert_eq!(stats.tree.nodes, 2 * stats.tree.leaves - 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn statistics(&self) -> Result<SceneStatistics, StatisticsError> {
        let (bounds, tree) = walk::tree_statistics(self.view())?;
        Ok(SceneStatistics {
            triangles: self.triangles.len(),
            triangles_by_material: self.triangles_by_material()?,
            smooth_triangles: self.smooth_triangles,
            zero_area_triangles: self.surfaces.iter().filter(|s| s.is_none()).count(),
            bounds,
            tree,
            memory: self.memory(),
        })
    }

    /// How many triangles name each material, in table order.
    fn triangles_by_material(&self) -> Result<Vec<usize>, StatisticsError> {
        let materials = self.materials.len();
        let mut counts = Vec::new();
        counts
            .try_reserve_exact(materials)
            .map_err(|error| StatisticsError::OutOfMemory { materials, error })?;
        counts.resize(materials, 0_usize);
        for &material in &self.material_of {
            // `Scene::new` checked every index against the table, so each one has its entry.
            if let Some(count) = counts.get_mut(widen(material)) {
                *count = count.saturating_add(1);
            }
        }
        Ok(counts)
    }

    /// The bytes of each buffer, `size_of_val` of its slice.
    fn memory(&self) -> SceneMemory {
        SceneMemory {
            triangles: size_of_val(self.triangles.as_slice()),
            surfaces: size_of_val(self.surfaces.as_slice()),
            vertex_normals: size_of_val(self.normals.as_slice()),
            material_indices: size_of_val(self.material_of.as_slice()),
            materials: size_of_val(self.materials.as_slice()),
            tree_nodes: size_of_val(self.tree.nodes.as_slice()),
            tree_primitive_indices: size_of_val(self.tree.prim_indices.as_slice()),
        }
    }
}
