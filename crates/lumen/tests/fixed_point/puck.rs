//! The ON puck as the camera sees it: a white disc with black lettering, not a black puck.

use bunny_num::FixedQ32_32;
use bunny_optics::PixelSample;
use lumen::scenes::fixed_point::{fixed_point_materials, Material, PartKind};
use lumen::{fixed_point_scene, render, ImageSize, RenderSettings, Tracer};

use crate::support::content;

/// The puck's colour a part shows: white face and side, or black rim and letters.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shade {
    White,
    Black,
}

/// What the camera ray through the centre of each pixel of a 480 by 270 image first hits, as
/// the puck's colour, and the rendered image (16 samples per pixel).
fn puck_as_seen() -> (Vec<Vec<Option<Shade>>>, lumen::Rgba8Image) {
    let scene = fixed_point_scene().expect("builds");
    let parts: Vec<_> = content().parts.into_iter().map(|p| (p.primitives, p.kind)).collect();
    let (width, height) = (480, 270);
    let camera = scene.camera().camera(width, height).expect("camera");
    let size = ImageSize::new(width, height).expect("size");
    let image =
        render(&scene, size, RenderSettings::new(16, 0).expect("settings")).expect("renders").image;
    let half = FixedQ32_32::from_raw(1 << 31);
    let shade = |x: u32, y: u32| {
        let ray = camera.ray(PixelSample { x, y, sub_x: half, sub_y: half }).expect("a ray");
        let index = scene.closest(&ray).expect("traces")?.primitive.to_usize();
        match parts.iter().find(|(range, _)| range.contains(&index)).map(|(_, k)| *k)? {
            PartKind::PuckFace | PartKind::PuckSide => Some(Shade::White),
            PartKind::PuckRim | PartKind::PuckLetters => Some(Shade::Black),
            _ => None,
        }
    };
    let grid = (0..height).map(|y| (0..width).map(|x| shade(x, y)).collect()).collect();
    (grid, image)
}

/// The number of pixels showing `shade`, and the number and mean rendered level (the mean of
/// red, green and blue) of those whose eight neighbours show it too, so that the pixel, whose
/// samples spread over its whole area, is not a blend of two colours at an edge.
fn measure(
    grid: &[Vec<Option<Shade>>],
    image: &lumen::Rgba8Image,
    shade: Shade,
) -> (u32, u32, u32) {
    let at = |x: usize, y: usize| grid.get(y).and_then(|row| row.get(x)).copied().flatten();
    let (mut all, mut inner, mut sum) = (0, 0, 0);
    for (y, row) in grid.iter().enumerate() {
        for (x, &here) in row.iter().enumerate() {
            if here != Some(shade) {
                continue;
            }
            all += 1;
            let near =
                |d: usize| [x.wrapping_sub(1), x, x + 1].map(|nx| (nx, (y + d).wrapping_sub(1)));
            let around = [near(0), near(1), near(2)];
            if around.iter().flatten().all(|&(nx, ny)| at(nx, ny) == Some(shade)) {
                let coordinates =
                    (u32::try_from(x).expect("fits"), u32::try_from(y).expect("fits"));
                let [r, g, b, _] = image.pixel(coordinates.0, coordinates.1).expect("in the image");
                inner += 1;
                sum += (u32::from(r) + u32::from(g) + u32::from(b)) / 3;
            }
        }
    }
    (all, inner, sum / inner.max(1))
}

/// Oracle: how an ON puck reads, a white disc with black lettering, stated as what the camera
/// sees. The white face and side are white and the rim and letters black (the albedos), at least
/// three times as many pixels show white puck as black, and the pixels wholly inside each colour
/// render bright and dark. (When recorded: 314 white and 75 black pixels, of which 114 and 4 are
/// wholly inside; means 230 and 53, a black albedo of 10/256 in sunlight through the sRGB curve.)
#[test]
fn the_camera_sees_a_white_puck_with_black_lettering() {
    let albedo = fixed_point_materials().expect("valid");
    let channels = |m: Material| albedo[usize::try_from(m.index()).expect("fits")].channels();
    assert_eq!(PartKind::PuckFace.material(), Material::White);
    assert_eq!(PartKind::PuckSide.material(), Material::White);
    assert_eq!(PartKind::PuckRim.material(), Material::Black);
    assert_eq!(PartKind::PuckLetters.material(), Material::Black);
    let ninety_percent = FixedQ32_32::from_raw((1 << 32) / 10 * 9);
    let five_percent = FixedQ32_32::from_raw((1 << 32) / 20);
    assert!(channels(Material::White).iter().all(|&c| c >= ninety_percent));
    assert!(channels(Material::Black).iter().all(|&c| c <= five_percent));
    let (grid, image) = puck_as_seen();
    let (white, white_inner, white_mean) = measure(&grid, &image, Shade::White);
    let (black, black_inner, black_mean) = measure(&grid, &image, Shade::Black);
    assert!(white >= 3 * black && black > 10, "{white} white and {black} black pixels");
    assert!(white_inner > 50 && black_inner >= 3, "{white_inner} and {black_inner} inner pixels");
    assert!(white_mean > 180 && black_mean < 80, "means {white_mean} and {black_mean}");
}
