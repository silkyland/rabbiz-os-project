use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use osk_core::gesture::GestureDetector;
use osk_core::layout::KeyPos;
use osk_core::scrub::SpacebarScrubber;
use osk_core::state::Keyboard;
use osk_render::{Renderer, Theme};

use smithay_client_toolkit::reexports::client::Connection;
use smithay_client_toolkit::{
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::wlr_layer::LayerSurface,
    shm::{
        slot::{Buffer, SlotPool},
        Shm,
    },
};

use wayland_protocols_misc::zwp_input_method_v2::client::zwp_input_method_v2::ZwpInputMethodV2;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1;

use crate::shell_bridge::ShellImClient;
use crate::touch::AltState;

pub(crate) struct App {
    pub(crate) reg: RegistryState,
    pub(crate) seats: SeatState,
    pub(crate) out: OutputState,
    pub(crate) shm: Shm,
    pub(crate) pool: SlotPool,
    pub(crate) layer: LayerSurface,
    pub(crate) im: ZwpInputMethodV2,
    /// `zwp_virtual_keyboard_v1` carries the keys an input method cannot express as text:
    /// Enter, and later arrows.
    pub(crate) virtual_keyboard: ZwpVirtualKeyboardV1,
    /// Number of `done` events so far; `commit(serial)` takes exactly this number.
    pub(crate) done_count: u32,
    pub(crate) keyboard: Keyboard,
    pub(crate) gesture: GestureDetector,
    pub(crate) scrubber: SpacebarScrubber,
    pub(crate) space_touch: Option<(i32, KeyPos)>,
    pub(crate) renderer: Renderer,
    pub(crate) theme: Theme,
    pub(crate) active: bool,
    pub(crate) shell_active: bool,
    pub(crate) has_surrounding_text: bool,
    pub(crate) user_visible: bool,
    pub(crate) configured: bool,
    pub(crate) visible: bool,
    /// Set when we asked the compositor for a fresh configure after being unmapped or resized.
    pub(crate) draw_pending: bool,
    pub(crate) w: u32,
    pub(crate) h: u32,
    pub(crate) held: Option<Buffer>,
    pub(crate) pressed: Vec<KeyPos>,
    pub(crate) alt: Option<AltState>,
    pub(crate) touch_pos: HashMap<i32, (f32, f32)>,
    pub(crate) strip_touches: HashSet<i32>,
    pub(crate) shell_im: Option<ShellImClient>,
    pub(crate) conn: Connection,
    /// `RABBIZ_HW_KEYBOARD` set: keep serving input-method-v3 but never map the window.
    pub(crate) hw_keyboard: bool,
    pub(crate) start: Instant,
    pub(crate) predictor_saved: Instant,
    /// K5: last selections seen by `wl-paste --watch`; memory only, never written to disk.
    pub(crate) clipboard: Arc<Mutex<Vec<String>>>,
    pub(crate) clipboard_changed: Arc<AtomicBool>,
    pub(crate) sheet: Option<Vec<String>>,
}

impl App {
    pub(crate) fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    pub(crate) fn layer_name(&self) -> String {
        self.keyboard.layer_name().to_string()
    }
}
