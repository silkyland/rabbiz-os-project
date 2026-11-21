//! `de-api` — the frozen seams of `rabbiz-de`.
//!
//! No smithay, no tiny-skia, no D-Bus: only data types and small traits so that
//! every lane (core, paint, ui, system, sim) compiles against the same contract
//! on any host. See `docs/DE-SEAMS.md` for the signature freeze and the
//! dependency rules.

#![forbid(unsafe_code)]

pub mod geom;
pub mod input;
pub mod orientation;
pub mod scene;
pub mod services;
pub mod stats;

pub use geom::{EdgeInsets, Point, Rect, Size, WindowInsets};
pub use input::{
    InputEvent, KeyEvent, KeyState, PointerEvent, SwitchEvent, TouchEvent, TouchPhase,
};
pub use orientation::{
    resolve as resolve_orientation, AccelSample, Orientation, OrientationDetector,
    OrientationRequest, RotationPolicy,
};
pub use scene::{DisplayArea, DisplayStack, Layer, LayerId, Pixels, Scene, Texture, TextureId};
pub use services::{
    BatteryState, BluetoothState, Notification, ServiceCommand, Services, ServicesSnapshot,
    VolumeState, WifiState,
};
pub use stats::{FrameStats, OutputInfo, RendererKind, StatsSummary, TimingSummary};
