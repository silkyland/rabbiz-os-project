//! System-service seams.
//!
//! `de-system` implements [`Services`] on a worker thread fed by
//! `std::sync::mpsc` — no async runtime crosses this seam (plan §M0-T2,
//! Decision 11). The UI/compositor reads a synchronous snapshot; mutations are
//! commands queued on the channel.

use std::sync::mpsc::Sender;

use serde::{Deserialize, Serialize};

/// Wi-Fi radios and connection state.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WifiState {
    #[default]
    Unknown,
    Disabled,
    Disconnected,
    Connecting,
    Connected {
        ssid: String,
        /// 0..=100 signal strength, if known.
        strength: Option<u8>,
    },
}

/// Bluetooth radio state.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BluetoothState {
    #[default]
    Unknown,
    Off,
    On,
    Connected {
        device_name: String,
    },
}

/// Audio output volume/mute snapshot.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeState {
    /// 0..=100 (may exceed 100 when the user over-amplifies; clamp at the UI).
    pub percent: u8,
    pub muted: bool,
    /// Sink/output name for the OSD label (e.g. "SMA1303 speaker").
    pub output_name: Option<String>,
}

/// Battery/charging snapshot (UPower).
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BatteryState {
    /// `None` when no battery is present.
    pub percent: Option<u8>,
    pub charging: bool,
    /// Fully-charged flag for the P20 "Fully charged" toast.
    pub full: bool,
}

/// Privacy indicators (M3-T1): a sensor is "in use" right now.
///
/// `de-system::privacy` polls the kernel/PipeWire state on its worker
/// thread; the UI only paints these booleans.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacyState {
    /// Camera stream open (`/dev/video*` held by a client).
    pub camera: bool,
    /// Microphone capture running.
    pub mic: bool,
    /// Location provider active.
    pub location: bool,
}

/// A single pending notification for the toast/center.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    /// Unix-ms timestamp when the notification arrived.
    pub time_ms: u64,
    /// D-Bus `replaces_id` grouping key.
    pub replaces_id: u32,
}

/// Everything the shell needs to paint the status bar / OSDs / shade.
///
/// Plain data, `Clone`, no `Rc`/`Arc` inside — safe to cross threads.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ServicesSnapshot {
    pub wifi: WifiState,
    pub bluetooth: BluetoothState,
    pub battery: BatteryState,
    pub volume: VolumeState,
    /// 0..=100 backlight level.
    pub brightness_pct: u8,
    /// Do-not-disturb master switch.
    pub dnd: bool,
    /// Airplane mode.
    pub airplane: bool,
    /// Active notifications (newest last).
    pub notifications: Vec<Notification>,
    /// MPRIS now-playing label, if any (`"artist — title"`).
    pub media: Option<String>,
    pub media_playing: bool,
    /// Camera/mic/location in-use flags (status-bar privacy dots).
    pub privacy: PrivacyState,
}

/// Mutations the UI can request; executed on the `de-system` worker thread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServiceCommand {
    SetBrightness(u8),
    SetVolume(u8),
    SetMuted(bool),
    SetWifiEnabled(bool),
    /// SSID + PSK. The worker must never log the PSK.
    ConnectWifi {
        ssid: String,
        psk: Option<String>,
    },
    SetBluetoothEnabled(bool),
    SetDnd(bool),
    SetAirplane(bool),
    DismissNotification(u32),
    ClearNotifications,
    MediaPlayPause,
    MediaNext,
    MediaPrev,
    /// Request a system action: `"lock"`, `"suspend"`, `"reboot"`, `"poweroff"`.
    Session(String),
}

/// Synchronous read seam: returns the latest snapshot. Must never block the
/// compositor — the implementor keeps a last-known snapshot it refreshes on a
/// worker thread.
pub trait Services: Send {
    fn snapshot(&self) -> ServicesSnapshot;
    fn commands(&self) -> &Sender<ServiceCommand>;
}
