//! P0 boot frame: the first pixels the panel shows.
//!
//! Frozen scaffold API: the scene is built from design tokens and returns
//! drawing operations only. No font is loaded here; `de-paint` owns text.

use de_api::{Scene, Size};
use de_design::DARK;

pub fn build(size: Size) -> Scene {
    let _ = (size, DARK);
    todo!("de-ui lane: build the plum brand frame from DARK tokens")
}
