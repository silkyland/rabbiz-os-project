//! Type scale from plan §7.17: bundled Noto Sans + Noto Sans Thai
//! (400/600), `tnum` for clocks/timers/percentages, micro tracking .08em.

/// A named text style: px size / px line-height / weight (+ tracking for
/// micro caps, + `tnum` flag for tabular numerals).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeStyle {
    /// Pixel font size.
    pub size: f32,
    /// Pixel line height.
    pub line_height: f32,
    /// 400 or 600 for the bundled faces.
    pub weight: u16,
    /// Extra letter-spacing in em (micro caps only).
    pub tracking_em: f32,
    /// Use tabular numerals (clocks, timers, percentages).
    pub tnum: bool,
}

const fn st(size: f32, line_height: f32, weight: u16) -> TypeStyle {
    TypeStyle {
        size,
        line_height,
        weight,
        tracking_em: 0.0,
        tnum: false,
    }
}

/// 104/112 w400 — the one huge moment (lock clock is 120/130 w300 per P1,
/// expressed via `TypeStyle::with`).
pub const DISPLAY: TypeStyle = st(104.0, 112.0, 400);
/// 40/48 w600.
pub const H1: TypeStyle = st(40.0, 48.0, 600);
/// 32/40 w600.
pub const H2: TypeStyle = st(32.0, 40.0, 600);
/// 28/36 w600.
pub const TITLE: TypeStyle = st(28.0, 36.0, 600);
/// 26/34 w400.
pub const BODY: TypeStyle = st(26.0, 34.0, 400);
/// 22/28 w600.
pub const LABEL: TypeStyle = st(22.0, 28.0, 600);
/// 20/26 w400.
pub const CAPTION: TypeStyle = st(20.0, 26.0, 400);
/// 18/24 w600 tracking .08em.
pub const MICRO: TypeStyle = TypeStyle {
    tracking_em: 0.08,
    ..st(18.0, 24.0, 600)
};

impl TypeStyle {
    /// Same metrics, different size/weight (e.g. P1 lock clock 120/130 w300).
    #[must_use]
    pub const fn with(self, size: f32, line_height: f32, weight: u16) -> Self {
        Self {
            size,
            line_height,
            weight,
            ..self
        }
    }

    /// Tabular-numeral variant (clocks, timers, percentages).
    #[must_use]
    pub const fn tnum(self) -> Self {
        Self { tnum: true, ..self }
    }
}

/// Scale factor for the tablet seam (all tokens x1.5 at 1080x2400).
pub const TABLET_SCALE: f32 = 1.5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::assertions_on_constants)] // spec conformance: asserts const API values
    fn scale_matches_spec() {
        assert_eq!(DISPLAY.size, 104.0);
        assert_eq!(BODY.weight, 400);
        assert_eq!(LABEL.size, 22.0);
        assert!((MICRO.tracking_em - 0.08).abs() < 1e-6);
        assert!(!MICRO.tnum);
        assert!(DISPLAY.tnum().tnum);
    }
}
