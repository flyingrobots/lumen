//! Shared readings of the canyon in grid steps, and the terrain, content and scene, each built
//! once for the whole test binary (the scene reads the 3 MB bunny and holds half a million
//! triangles).

use std::sync::OnceLock;

use bunny_linalg::FixedVec3;
use lumen::scenes::canyon::heightfield::Heightfield;
use lumen::scenes::canyon::terrain::{canyon_heights, grid_point, CELL, COLUMNS, ORIGIN, ROWS};
use lumen::scenes::canyon::{canyon_content, CanyonContent};
use lumen::{canyon_scene, Scene};

/// One grid step in raw Q32.32 units.
pub(crate) const STEP_RAW: i64 = 1 << 16;

/// A point in grid steps.
pub(crate) type Steps = [i64; 3];

/// A vertex in grid steps; every scene coordinate is a whole number of steps.
pub(crate) fn steps(v: FixedVec3) -> Steps {
    [v.x, v.y, v.z].map(|c| {
        assert_eq!(c.raw() % STEP_RAW, 0, "{c:?} is off the 2^-16 grid");
        c.raw() / STEP_RAW
    })
}

/// The box around `points`: the lowest and highest of each coordinate.
pub(crate) fn bounds(points: impl Iterator<Item = Steps>) -> [Steps; 2] {
    points.fold([[i64::MAX; 3], [i64::MIN; 3]], |[low, high], p| {
        [[0, 1, 2].map(|k| low[k].min(p[k])), [0, 1, 2].map(|k| high[k].max(p[k]))]
    })
}

/// The terrain's heights; they generate.
pub(crate) fn heights() -> &'static Heightfield {
    static HEIGHTS: OnceLock<Heightfield> = OnceLock::new();
    HEIGHTS.get_or_init(|| canyon_heights().expect("the canyon's heights generate"))
}

/// The terrain's grid vertices, in grid steps, of every cell that overlaps the box from `low`
/// to `high` on X and Z: the corners of every terrain triangle that can reach into the box.
pub(crate) fn terrain_under(low: Steps, high: Steps) -> Vec<Steps> {
    let (x0, z0) = ORIGIN;
    let column = |x: i64| (x - x0).div_euclid(CELL.0);
    let row = |z: i64| (z0 - z).div_euclid(CELL.1);
    let columns = column(low[0]).max(0)..=(column(high[0]) + 1).min(i64::from(COLUMNS) - 1);
    let rows = row(high[2]).max(0)..=(row(low[2]) + 1).min(i64::from(ROWS) - 1);
    let mut out = Vec::new();
    for j in rows {
        for i in columns.clone() {
            let (i, j) = (u32::try_from(i).expect("on"), u32::try_from(j).expect("on"));
            let height = heights().height(i, j).expect("on the grid");
            out.push(steps(grid_point(i, j, height)));
        }
    }
    out
}

/// The scene's content; it builds.
pub(crate) fn content() -> &'static CanyonContent {
    static CONTENT: OnceLock<CanyonContent> = OnceLock::new();
    CONTENT.get_or_init(|| canyon_content().expect("the canyon content builds"))
}

/// The scene; it builds.
pub(crate) fn scene() -> &'static Scene {
    static SCENE: OnceLock<Scene> = OnceLock::new();
    SCENE.get_or_init(|| canyon_scene().expect("the canyon scene builds"))
}
