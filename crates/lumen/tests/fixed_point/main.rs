//! Size class: small (in process, no file, no network).
//!
//! Tests of the Fixed Point scene (`lumen-fixed-point-scene/2`) through lumen's public API:
//! the dice, the table and puck, the sunglasses' fit, the scene's integrity and support, and
//! rendering (every traced ray against brute force, and the golden hash).

mod dice;
mod glasses;
#[path = "../golden/mod.rs"]
mod golden;
mod integrity;
mod puck;
mod rendering;
mod support;
mod table;
