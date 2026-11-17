//! Retained scene: layers of drawing operations in device pixels.

use crate::geom::{Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    pub fn to_rgba8(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Regular,
    SemiBold,
}

impl Weight {
    pub const fn numeric(self) -> u16 {
        match self {
            Weight::Regular => 400,
            Weight::SemiBold => 600,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub weight: Weight,
    pub color: Color,
    pub max_width: f32,
}

impl TextStyle {
    pub const fn new(size: f32, weight: Weight, color: Color, max_width: f32) -> Self {
        Self { size, weight, color, max_width }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DrawOp {
    /// Filled rectangle. `radius` is clamped to half the shorter side.
    Rect { rect: Rect, radius: f32, color: Color },
    /// Text box; `origin` is the top-left corner of the box.
    Text { origin: Point, text: String, style: TextStyle },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub ops: Vec<DrawOp>,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ops: Vec::new() }
    }

    pub fn rect(&mut self, rect: Rect, radius: f32, color: Color) {
        self.ops.push(DrawOp::Rect { rect, radius, color });
    }

    pub fn text(&mut self, origin: Point, text: impl Into<String>, style: TextStyle) {
        self.ops.push(DrawOp::Text { origin, text: text.into(), style });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub size: Size,
    pub layers: Vec<Layer>,
}

impl Scene {
    pub fn new(size: Size) -> Self {
        Self { size, layers: Vec::new() }
    }

    pub fn push(&mut self, layer: Layer) {
        self.layers.push(layer);
    }
}
