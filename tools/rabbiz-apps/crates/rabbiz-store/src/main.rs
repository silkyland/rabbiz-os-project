//! RabbizOS Store & System Updater (`rabbiz-store`) — First-party native Rust
//! application for updating the Linux kernel, device/DTB/driver patches, and
//! browsing/installing mobile-friendly apps from Recommended (`.rbz` / native),
//! Flatpak (Flathub ARM64), and Paru (Arch Linux ARM / AUR).

#![forbid(unsafe_code)]

mod backend;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};

use de_api::{Point, Rect, Size};
use rabbiz_app_kit::mcp::Tool;
use rabbiz_app_kit::{
    icons, text, tokens, ui, widgets, AppEvent, AppFrame, AppState, Canvas, ScrollView,
    TextBuffer, TextInputCtl, Theme,
};
use serde::{Deserialize, Serialize};

/// Material Icons Round glyphs not (yet) exported by `rabbiz_app_kit::icons`.
/// Verified against the bundled `MaterialIconsRound-Regular.ttf` cmap.
const ICON_TERMINAL: &str = "\u{eb8e}";
const ICON_BUILD: &str = "\u{e869}";

/// Store list metrics — the single source shared by `item_card_rects`,
/// `hit_test` and `render` so content height can't drift (audit S8).
const STRIP_H: i32 = 44;
const HERO_H: i32 = 168;
const HERO_GAP: i32 = 12;
const ROW_H: i32 = 100;
const ROW_ACTION_W: i32 = 132;
const ICON_TILE: i32 = 64;

/// Top navigation tabs inside `rabbiz-store`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreTab {
    Updates,
    Recommended,
    Flatpak,
    Paru,
}

impl StoreTab {
    pub const ALL: [Self; 4] = [
        Self::Updates,
        Self::Recommended,
        Self::Flatpak,
        Self::Paru,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Updates => "Updates",
            Self::Recommended => "Featured",
            Self::Flatpak => "Flatpak",
            Self::Paru => "Paru AUR",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "updates" | "update" | "linux" | "patch" => Some(Self::Updates),
            "recommended" | "featured" | "rabbiz" => Some(Self::Recommended),
            "flatpak" | "flathub" => Some(Self::Flatpak),
            "paru" | "aur" | "pacman" | "arch" => Some(Self::Paru),
            _ => None,
        }
    }
}

/// Package source backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageSource {
    LinuxPatch,
    Recommended,
    Flatpak,
    Paru,
}

impl PackageSource {
    #[must_use]
    pub const fn badge(self) -> &'static str {
        match self {
            Self::LinuxPatch => "KERNEL / PATCH",
            Self::Recommended => "RABBIZ CURATED",
            Self::Flatpak => "FLATPAK · ARM64",
            Self::Paru => "PARU · ARCH/AUR",
        }
    }

    #[must_use]
    pub fn cli_preview(self, pkg_id: &str, install: bool) -> String {
        match (self, install) {
            (Self::LinuxPatch, _) => format!("rabbiz-updater apply {pkg_id}"),
            (Self::Recommended, true) => format!("rabbiz install {pkg_id}.rbz"),
            (Self::Recommended, false) => format!("rabbiz uninstall {pkg_id}"),
            (Self::Flatpak, true) => format!("flatpak install -y flathub {pkg_id}"),
            (Self::Flatpak, false) => format!("flatpak uninstall -y {pkg_id}"),
            (Self::Paru, true) => format!("paru -S --noconfirm {pkg_id}"),
            (Self::Paru, false) => format!("paru -Rns --noconfirm {pkg_id}"),
        }
    }
}

/// One Linux kernel / driver patch or store application item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogItem {
    pub id: String,
    pub name: String,
    pub version: String,
    pub summary: String,
    pub source: PackageSource,
    pub size_label: String,
    pub installed: bool,
    pub update_available: bool,
}

/// Persistent state saved to `$XDG_DATA_HOME/rabbiz-store/state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedStoreState {
    pub kernel_release: String,
    pub os_channel: String,
    pub last_checked: String,
    pub items: Vec<CatalogItem>,
}

fn state_file_path() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".local/share")
        });
    base.join("rabbiz-store").join("state.json")
}

fn probe_kernel_release() -> String {
    if let Ok(out) = Command::new("uname").arg("-r").output() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    "7.3.0-rabbiz-exynos850".to_string()
}

#[must_use]
pub fn default_catalog_items() -> Vec<CatalogItem> {
    vec![
        // --- Tab 1: Linux Kernel, Device Tree & Driver Patches ---
        CatalogItem {
            id: "linux-mainline-7.3.0-a21s-r2".into(),
            name: "Mainline Linux 7.3.0 + Exynos 850 DTB Patchset".into(),
            version: "7.3.0-2 -> 7.3.0-3".into(),
            summary: "Updates exynos850-a217f.dtb (TMU eFuse thermal trip points, decon/dsi & S2MPU12 rail guards)".into(),
            source: PackageSource::LinuxPatch,
            size_label: "28.4 MB".into(),
            installed: true,
            update_available: true,
        },
        CatalogItem {
            id: "scsc-wlbt-mainline-patch".into(),
            name: "SCSC Maxwell Wi-Fi & BlueZ H4 Driver Patch".into(),
            version: "2026.11.1 -> 2026.11.2".into(),
            summary: "Out-of-tree scsc_wlan.ko & scsc_bt.ko with mx140 recovery guard and H4 skb fix".into(),
            source: PackageSource::LinuxPatch,
            size_label: "4.2 MB".into(),
            installed: true,
            update_available: true,
        },
        CatalogItem {
            id: "abox-calliope-dsp-patch".into(),
            name: "Exynos ABOX Calliope DSP + SMA1303 Audio Patch".into(),
            version: "1.4.0 -> 1.5.0".into(),
            summary: "256 KiB PCM buffer cap, scatter-gather firmware DMA fix & PipeWire UCM2 profile".into(),
            source: PackageSource::LinuxPatch,
            size_label: "3.1 MB".into(),
            installed: true,
            update_available: true,
        },
        CatalogItem {
            id: "s2mu106-power-charger-patch".into(),
            name: "S2MU106 Fuel Gauge & Charger Service".into(),
            version: "1.1.0 -> 1.2.0".into(),
            summary: "Exposes real voltage_now/current_now/capacity to UPower and enables boot charging".into(),
            source: PackageSource::LinuxPatch,
            size_label: "1.4 MB".into(),
            installed: true,
            update_available: true,
        },
        CatalogItem {
            id: "rabbiz-de-wayland-compositor".into(),
            name: "RabbizOS Desktop Environment (rabbiz-de + OSK)".into(),
            version: "0.1.0 -> 0.2.0".into(),
            summary: "14-crate Rust Smithay compositor, 60 FPS morphs, Thai/EN keyboard & Paravirtual bridge".into(),
            source: PackageSource::LinuxPatch,
            size_label: "12.8 MB".into(),
            installed: true,
            update_available: true,
        },

        // --- Tab 2: Recommended / Curated for RabbizOS Mobile ---
        CatalogItem {
            id: "org.kde.index".into(),
            name: "Index File Manager (MauiKit Touch)".into(),
            version: "4.0.0".into(),
            summary: "Convergent touch-friendly file manager tailored for mobile Linux handsets".into(),
            source: PackageSource::Recommended,
            size_label: "14.2 MB".into(),
            installed: true,
            update_available: false,
        },
        CatalogItem {
            id: "org.kde.merkuro.calendar".into(),
            name: "Merkuro Calendar & Contacts".into(),
            version: "24.08.3".into(),
            summary: "Kirigami mobile calendar, agenda planner, and address book for RabbizOS".into(),
            source: PackageSource::Recommended,
            size_label: "18.6 MB".into(),
            installed: false,
            update_available: false,
        },
        CatalogItem {
            id: "org.gnome.Calls".into(),
            name: "GNOME Calls (oFono / ModemManager)".into(),
            version: "47.0".into(),
            summary: "Mobile PSTN/cellular phone dialer and call handler with callaudiod routing".into(),
            source: PackageSource::Recommended,
            size_label: "9.4 MB".into(),
            installed: false,
            update_available: false,
        },
        CatalogItem {
            id: "sm.puri.Chatty".into(),
            name: "Chatty Messaging (SMS / MMS / Matrix)".into(),
            version: "0.8.5".into(),
            summary: "Libhandy/GTK4 mobile messaging client for cellular SMS and encrypted chat".into(),
            source: PackageSource::Recommended,
            size_label: "11.8 MB".into(),
            installed: false,
            update_available: false,
        },
        CatalogItem {
            id: "com.devded.rabbiz-weather".into(),
            name: "Rabbiz Weather (.rbz SDK Package)".into(),
            version: "0.1.0".into(),
            summary: "First-party plum/copper mobile weather forecast & air quality card app".into(),
            source: PackageSource::Recommended,
            size_label: "2.1 MB".into(),
            installed: false,
            update_available: false,
        },

        // --- Tab 3: Flatpak (Flathub ARM64) ---
        CatalogItem {
            id: "org.mozilla.firefox".into(),
            name: "Firefox Mobile / Wayland (Flathub)".into(),
            version: "132.0.2".into(),
            summary: "Mozilla Firefox ARM64 with Wayland touch gestures and mobile user-agent profile".into(),
            source: PackageSource::Flatpak,
            size_label: "94.0 MB".into(),
            installed: false,
            update_available: false,
        },
        CatalogItem {
            id: "org.telegram.desktop".into(),
            name: "Telegram Desktop ARM64 (Flathub)".into(),
            version: "5.7.2".into(),
            summary: "Fast cloud messaging client with adaptive responsive single-column mobile layout".into(),
            source: PackageSource::Flatpak,
            size_label: "68.5 MB".into(),
            installed: false,
            update_available: false,
        },
        CatalogItem {
            id: "org.videolan.VLC".into(),
            name: "VLC Media Player (Flathub)".into(),
            version: "3.0.21".into(),
            summary: "Universal multimedia player supporting hardware ARM64 NEON & PipeWire audio".into(),
            source: PackageSource::Flatpak,
            size_label: "52.0 MB".into(),
            installed: false,
            update_available: false,
        },
        CatalogItem {
            id: "com.spotify.Client".into(),
            name: "Spotify Music + Rabbiz MPRIS Bridge".into(),
            version: "1.2.50".into(),
            summary: "Stream music with Quick Shade media controls via rabbiz-spotify-ctl".into(),
            source: PackageSource::Flatpak,
            size_label: "86.0 MB".into(),
            installed: false,
            update_available: false,
        },
        CatalogItem {
            id: "org.kde.kweather".into(),
            name: "KWeather Mobile (Flathub)".into(),
            version: "24.08.3".into(),
            summary: "Convergent Kirigami weather application for Plasma Mobile and RabbizOS".into(),
            source: PackageSource::Flatpak,
            size_label: "15.4 MB".into(),
            installed: false,
            update_available: false,
        },

        // --- Tab 4: Paru (Arch Linux ARM / AUR) ---
        CatalogItem {
            id: "paru-bin".into(),
            name: "paru (Rust AUR Helper & Pacman Wrapper)".into(),
            version: "2.0.4-1".into(),
            summary: "Feature-packed Rust AUR helper and Arch Linux ARM package manager backend".into(),
            source: PackageSource::Paru,
            size_label: "3.4 MB".into(),
            installed: true,
            update_available: false,
        },
        CatalogItem {
            id: "fastfetch".into(),
            name: "fastfetch (Arch Linux ARM extra)".into(),
            version: "2.29.0-1".into(),
            summary: "High-performance system information tool showing Exynos 850 & Panfrost GPU stats".into(),
            source: PackageSource::Paru,
            size_label: "1.8 MB".into(),
            installed: true,
            update_available: false,
        },
        CatalogItem {
            id: "htop".into(),
            name: "htop Interactive Process Viewer".into(),
            version: "3.3.0-3".into(),
            summary: "Monitor 8x Cortex-A55 cores, ZRAM swap, and CMA memory headroom in rabbiz-terminal".into(),
            source: PackageSource::Paru,
            size_label: "0.9 MB".into(),
            installed: true,
            update_available: false,
        },
        CatalogItem {
            id: "box64-git".into(),
            name: "box64-git (ARM64 x86_64 Userspace Emulator — AUR)".into(),
            version: "0.3.1.r120-1".into(),
            summary: "Run x86_64 Linux binaries and games on ARM64 with Dynarec acceleration".into(),
            source: PackageSource::Paru,
            size_label: "16.2 MB".into(),
            installed: false,
            update_available: false,
        },
        CatalogItem {
            id: "waydroid-arm64".into(),
            name: "waydroid (Containerized Android Subsystem — AUR)".into(),
            version: "1.4.3-1".into(),
            summary: "Optional isolated LXC container for running legacy Android APKs inside Wayland".into(),
            source: PackageSource::Paru,
            size_label: "24.0 MB".into(),
            installed: false,
            update_available: false,
        },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    None,
    Tab(StoreTab),
    SearchBar,
    ClearSearch,
    ToggleLog,
    ClearLog,
    CloseLog,
    CheckUpdatesBtn,
    ApplyAllUpdatesBtn,
    ItemAction(usize),
    ItemOpen(usize),
    DetailBack,
    DetailAction(usize),
    DialogAction(usize),
    DialogDismiss,
}

pub struct StoreApp {
    pub tab: StoreTab,
    pub search: TextBuffer,
    pub search_focused: bool,
    pub kernel_release: String,
    pub os_channel: String,
    pub last_checked: String,
    pub status_banner: String,
    pub last_cmd_log: String,
    pub show_log: bool,
    pub log_lines: VecDeque<String>,
    pub items: Vec<CatalogItem>,
    pub scroll_y: i32,
    pub content_h: i32,
    size: Size,
    state_path: PathBuf,
    pressed: Hit,
    scroll: ScrollView,
    scroll_touch: Option<i32>,
    epoch: std::time::Instant,
    anim_t: f32,
    busy_item: Option<String>,
    detail: Option<usize>,
    confirm_uninstall: Option<usize>,
    busy_job: Option<String>,
    job_tx: Sender<backend::JobEvent>,
    job_rx: Receiver<backend::JobEvent>,
}

impl Default for StoreApp {
    fn default() -> Self {
        Self::new()
    }
}

impl StoreApp {
    #[must_use]
    pub fn new() -> Self {
        Self::with_state_path(state_file_path())
    }

    fn with_state_path(state_path: PathBuf) -> Self {
        let (job_tx, job_rx) = channel();
        if let Ok(raw) = std::fs::read_to_string(&state_path) {
            if let Ok(saved) = serde_json::from_str::<PersistedStoreState>(&raw) {
                return Self {
                    tab: StoreTab::Updates,
                    search: TextBuffer::new(),
                    search_focused: false,
                    kernel_release: saved.kernel_release,
                    os_channel: saved.os_channel,
                    last_checked: saved.last_checked,
                    status_banner: "Ready · Linux Patch Updater + Flatpak + Paru Store".into(),
                    last_cmd_log: "paru -Syu / flatpak update ready".into(),
                    show_log: false,
                    log_lines: VecDeque::from([
                        "RabbizOS Store & Updater ready.".to_string(),
                        "Tap the terminal icon to inspect live paru / flatpak / package output.".to_string(),
                    ]),
                    items: saved.items,
                    scroll_y: 0,
                    content_h: 0,
                    size: Size::new(720, 1600),
                    state_path,
                    pressed: Hit::None,
                    scroll: ScrollView::new(),
                    scroll_touch: None,
                    epoch: std::time::Instant::now(),
                    anim_t: 0.0,
                    busy_item: None,
                    detail: None,
                    confirm_uninstall: None,
                    busy_job: None,
                    job_tx,
                    job_rx,
                };
            }
        }
        let mut s = Self {
            tab: StoreTab::Updates,
            search: TextBuffer::new(),
            search_focused: false,
            kernel_release: probe_kernel_release(),
            os_channel: "RabbizOS 2026.11 (Arch Linux ARM64 · Exynos 850)".into(),
            last_checked: "Just now".into(),
            status_banner: "5 Linux & driver patches ready to install".into(),
            last_cmd_log: "rabbiz-updater check --channel=mainline-7.3".into(),
            show_log: false,
            log_lines: VecDeque::from([
                "RabbizOS Store & Updater ready.".to_string(),
                "Tap the terminal icon to inspect live paru / flatpak / package output.".to_string(),
            ]),
            items: default_catalog_items(),
            scroll_y: 0,
            content_h: 0,
            size: Size::new(720, 1600),
            state_path,
            pressed: Hit::None,
            scroll: ScrollView::new(),
            scroll_touch: None,
            epoch: std::time::Instant::now(),
            anim_t: 0.0,
            busy_item: None,
            detail: None,
            confirm_uninstall: None,
            busy_job: None,
            job_tx,
            job_rx,
        };
        s.sync_installed_flags();
        s.save_state();
        s
    }

    /// Reconcile `installed` state against actual binaries and `.desktop` entries on this system.
    pub fn sync_installed_flags(&mut self) {
        if backend::dry_run() {
            return;
        }
        for item in &mut self.items {
            if item.source == PackageSource::LinuxPatch {
                continue;
            }
            if backend::desktop_exists(&item.id)
                || (item.id == "htop" && backend::available("htop"))
                || (item.id == "fastfetch"
                    && (backend::available("fastfetch") || backend::available("neofetch")))
                || (item.id == "paru-bin" && backend::available("paru"))
            {
                item.installed = true;
            }
        }
    }

    pub fn save_state(&self) {
        let path = &self.state_path;
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let state = PersistedStoreState {
            kernel_release: self.kernel_release.clone(),
            os_channel: self.os_channel.clone(),
            last_checked: self.last_checked.clone(),
            items: self.items.clone(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            let tmp = path.with_extension("tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }

    #[must_use]
    pub fn pending_updates_count(&self) -> usize {
        self.items.iter().filter(|i| i.update_available).count()
    }

    /// Indices into `self.items` visible under the active tab and search query.
    #[must_use]
    pub fn visible_indices(&self) -> Vec<usize> {
        let q = self.search.as_str().trim().to_ascii_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                let tab_match = match self.tab {
                    StoreTab::Updates => {
                        item.source == PackageSource::LinuxPatch || item.update_available
                    }
                    StoreTab::Recommended => item.source == PackageSource::Recommended,
                    StoreTab::Flatpak => item.source == PackageSource::Flatpak,
                    StoreTab::Paru => item.source == PackageSource::Paru,
                };
                if !tab_match {
                    return false;
                }
                if q.is_empty() {
                    return true;
                }
                item.name.to_ascii_lowercase().contains(&q)
                    || item.id.to_ascii_lowercase().contains(&q)
                    || item.summary.to_ascii_lowercase().contains(&q)
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    fn push_log_line(&mut self, line: String) {
        let trimmed = line.trim_end().to_string();
        if trimmed.is_empty() {
            return;
        }
        if self.log_lines.len() >= 120 {
            self.log_lines.pop_front();
        }
        self.log_lines.push_back(trimmed);
    }

    /// Drain finished or streaming background package-manager job events.
    pub fn drain_jobs(&mut self) -> bool {
        let mut changed = false;
        while let Ok(ev) = self.job_rx.try_recv() {
            changed = true;
            match ev {
                backend::JobEvent::Line(line) => {
                    if !line.trim().is_empty() {
                        self.last_cmd_log = line.trim().to_string();
                    }
                    self.push_log_line(line);
                }
                backend::JobEvent::Done(res) => {
                    self.busy_job = None;
                    self.busy_item = None;
                    if res.ok {
                        if let (Some(id), Some(want_install)) =
                            (&res.item_id, res.install_intent)
                        {
                            if let Some(item) = self.items.iter_mut().find(|i| i.id == *id) {
                                item.installed = want_install;
                                if want_install && item.update_available {
                                    item.update_available = false;
                                    if let Some((_, new_ver)) = item.version.split_once("->") {
                                        item.version = new_ver.trim().to_string();
                                    }
                                }
                            }
                        } else if res.label == "Linux & System Updates" {
                            for item in &mut self.items {
                                if item.update_available {
                                    item.update_available = false;
                                    item.installed = true;
                                    if let Some((_, new_ver)) = item.version.split_once("->") {
                                        item.version = new_ver.trim().to_string();
                                    }
                                }
                            }
                        }
                        let summary_line = res
                            .stdout
                            .lines()
                            .rev()
                            .find(|l| !l.trim().is_empty())
                            .map(str::trim)
                            .unwrap_or("");
                        self.status_banner =
                            if !summary_line.is_empty() && summary_line.len() <= 52 {
                                format!("{}: {summary_line}", res.label)
                            } else {
                                let n = backend::count_lines(&res.stdout);
                                if n > 0 {
                                    format!("{} done ({n} lines)", res.label)
                                } else {
                                    format!("{} completed cleanly", res.label)
                                }
                            };
                    } else {
                        let err = res
                            .stderr
                            .lines()
                            .chain(res.stdout.lines())
                            .find(|l| !l.trim().is_empty())
                            .unwrap_or("command failed")
                            .trim();
                        self.status_banner = format!("{} failed: {err}", res.label);
                    }
                    self.save_state();
                }
            }
        }
        changed
    }

    /// Check for new Linux kernel/driver patches and refresh metadata.
    pub fn check_for_updates(&mut self) {
        self.last_checked = "Synced just now".into();
        self.busy_item = None;
        let pending = self.pending_updates_count();
        if backend::available("paru") {
            self.busy_job = Some("Checking updates".into());
            self.last_cmd_log = "paru -Qu && flatpak remote-ls --updates".into();
            self.status_banner = "Checking Arch Linux ARM (paru) & Flatpak…".into();
            backend::spawn_command_job(
                None,
                None,
                "Check updates".into(),
                vec![backend::CommandStep::new("paru", ["-Qu"]).allow_nonzero()],
                false,
                self.job_tx.clone(),
            );
        } else if backend::available("apt-get") {
            self.busy_job = Some("Checking updates".into());
            self.last_cmd_log = "apt-get update && apt list --upgradable".into();
            self.status_banner = "Checking Linux packages (apt-get)…".into();
            backend::spawn_command_job(
                None,
                None,
                "Check updates".into(),
                vec![
                    backend::CommandStep::new("apt-get", ["update", "-qq"])
                        .env("DEBIAN_FRONTEND", "noninteractive"),
                    backend::CommandStep::new("apt", ["list", "--upgradable"]),
                ],
                false,
                self.job_tx.clone(),
            );
        } else if pending == 0 {
            self.last_cmd_log = "rabbiz-updater --check".into();
            self.status_banner = "System is up to date (Kernel 7.3.0-3 + all patches)".into();
        } else {
            self.last_cmd_log = "rabbiz-updater --check".into();
            self.status_banner = format!("{pending} Linux kernel & driver patches available");
        }
        self.save_state();
    }

    /// Apply all pending Linux kernel, DTB, and driver patches.
    pub fn apply_all_updates(&mut self) -> usize {
        self.busy_item = None;
        if backend::available("paru") {
            self.busy_job = Some("Linux & System Updates".into());
            self.last_cmd_log = "paru -Syu --noconfirm && flatpak update -y".into();
            self.status_banner = "Updating Linux & packages via paru…".into();
            let mut steps = vec![backend::CommandStep::new("paru", ["-Syu", "--noconfirm"])];
            if backend::available("flatpak") {
                steps.push(backend::CommandStep::new("flatpak", ["update", "-y"]));
            }
            backend::spawn_command_job(
                None,
                None,
                "Linux & System Updates".into(),
                steps,
                true,
                self.job_tx.clone(),
            );
            return self.pending_updates_count();
        } else if backend::available("apt-get") {
            self.busy_job = Some("Linux & System Updates".into());
            self.last_cmd_log = "apt-get update && apt-get upgrade -y".into();
            self.status_banner = "Updating Linux system packages (apt-get)…".into();
            backend::spawn_command_job(
                None,
                None,
                "Linux & System Updates".into(),
                vec![
                    backend::CommandStep::new("apt-get", ["update", "-qq"])
                        .env("DEBIAN_FRONTEND", "noninteractive"),
                    backend::CommandStep::new("apt-get", ["upgrade", "-y"])
                        .env("DEBIAN_FRONTEND", "noninteractive"),
                ],
                true,
                self.job_tx.clone(),
            );
            return self.pending_updates_count();
        }

        let mut applied = 0;
        for item in &mut self.items {
            if item.update_available {
                item.update_available = false;
                item.installed = true;
                if let Some((_, new_ver)) = item.version.split_once("->") {
                    item.version = new_ver.trim().to_string();
                }
                applied += 1;
            }
        }
        if applied > 0 {
            self.kernel_release = "7.3.0-3-rabbiz-a21s".into();
            self.status_banner = format!("Applied {applied} Linux & driver patches cleanly");
            self.last_cmd_log = "rabbiz-updater apply-all && paru -Syu --noconfirm".into();
        } else {
            self.status_banner = "All Linux patches are already up to date".into();
        }
        self.save_state();
        applied
    }

    /// Toggle install/update/remove on `self.items[idx]`.
    pub fn trigger_item_action(&mut self, idx: usize) {
        let Some(item) = self.items.get(idx).cloned() else {
            return;
        };
        if !backend::is_valid_pkg_id(&item.id) {
            self.status_banner = format!("Rejected invalid package id: {}", item.id);
            return;
        }
        let want_install = item.update_available || !item.installed;
        self.busy_item = None;
        let cmd_preview = item.source.cli_preview(&item.id, want_install);

        // 1. Curated first-party .rbz app (`com.devded.rabbiz-weather`) installs real .desktop + SVG!
        if item.id == "com.devded.rabbiz-weather" && !backend::dry_run() {
            match backend::install_curated_rbz_app(&item.id, &item.name, want_install) {
                Ok(msg) => {
                    if let Some(it) = self.items.get_mut(idx) {
                        it.installed = want_install;
                        it.update_available = false;
                    }
                    self.last_cmd_log = cmd_preview;
                    self.status_banner = msg;
                }
                Err(err) => {
                    self.status_banner = format!("Failed: {err}");
                }
            }
            self.save_state();
            return;
        }

        // 2. Real Flatpak backend (auto-bootstraps flatpak + flathub if apt-get/paru is present)
        if item.source == PackageSource::Flatpak {
            if backend::available("flatpak") {
                let steps = if want_install {
                    vec![
                        backend::CommandStep::new(
                            "flatpak",
                            [
                                "remote-add",
                                "--if-not-exists",
                                "flathub",
                                "https://dl.flathub.org/repo/flathub.flatpakrepo",
                            ],
                        ),
                        backend::CommandStep::new(
                            "flatpak",
                            ["install", "-y", "--noninteractive", "flathub", "--", &item.id],
                        ),
                    ]
                } else {
                    vec![backend::CommandStep::new(
                        "flatpak",
                        ["uninstall", "-y", "--noninteractive", "--", &item.id],
                    )]
                };
                self.busy_job = Some(item.name.clone());
                self.busy_item = Some(item.id.clone());
                self.last_cmd_log = cmd_preview;
                self.status_banner = format!("Running Flatpak for {}…", item.name);
                backend::spawn_command_job(
                    Some(item.id),
                    Some(want_install),
                    item.name,
                    steps,
                    true,
                    self.job_tx.clone(),
                );
                return;
            } else if want_install && backend::available("apt-get") {
                let steps = vec![
                    backend::CommandStep::new("apt-get", ["update", "-qq"])
                        .env("DEBIAN_FRONTEND", "noninteractive"),
                    backend::CommandStep::new("apt-get", ["install", "-y", "flatpak"])
                        .env("DEBIAN_FRONTEND", "noninteractive"),
                    backend::CommandStep::new(
                        "flatpak",
                        [
                            "remote-add",
                            "--if-not-exists",
                            "flathub",
                            "https://dl.flathub.org/repo/flathub.flatpakrepo",
                        ],
                    ),
                    backend::CommandStep::new(
                        "flatpak",
                        ["install", "-y", "--noninteractive", "flathub", "--", &item.id],
                    ),
                ];
                self.busy_job = Some(item.name.clone());
                self.busy_item = Some(item.id.clone());
                self.last_cmd_log = format!(
                    "apt-get install flatpak && flatpak install flathub {}",
                    item.id
                );
                self.status_banner = format!("Installing Flatpak + {}…", item.name);
                backend::spawn_command_job(
                    Some(item.id),
                    Some(true),
                    item.name,
                    steps,
                    true,
                    self.job_tx.clone(),
                );
                return;
            }
        }

        // 3. Real Paru (Arch Linux ARM) or APT (Debian ARM64 container) backend
        if matches!(item.source, PackageSource::Paru | PackageSource::Recommended) {
            if backend::available("paru") {
                let pkg = backend::map_arch_pkg(&item.id);
                if !backend::is_valid_pkg_id(pkg) {
                    self.status_banner = format!("Rejected invalid package name: {pkg}");
                    return;
                }
                let (preview, steps) = if want_install {
                    (
                        format!("paru -S --noconfirm -- {pkg}"),
                        vec![backend::CommandStep::new(
                            "paru",
                            ["-S", "--noconfirm", "--", pkg],
                        )],
                    )
                } else {
                    (
                        format!("paru -Rns --noconfirm -- {pkg}"),
                        vec![backend::CommandStep::new(
                            "paru",
                            ["-Rns", "--noconfirm", "--", pkg],
                        )],
                    )
                };
                self.busy_job = Some(item.name.clone());
                self.busy_item = Some(item.id.clone());
                self.last_cmd_log = preview.clone();
                self.status_banner = format!("Running {preview}…");
                backend::spawn_command_job(
                    Some(item.id),
                    Some(want_install),
                    item.name,
                    steps,
                    true,
                    self.job_tx.clone(),
                );
                return;
            } else if backend::available("apt-get") {
                if let Some(deb_pkg) = backend::map_debian_pkg(&item.id) {
                    let steps = if want_install {
                        vec![
                            backend::CommandStep::new("apt-get", ["update", "-qq"])
                                .env("DEBIAN_FRONTEND", "noninteractive"),
                            backend::CommandStep::new("apt-get", ["install", "-y", "--", deb_pkg])
                                .env("DEBIAN_FRONTEND", "noninteractive"),
                        ]
                    } else {
                        vec![
                            backend::CommandStep::new("apt-get", ["remove", "-y", "--", deb_pkg])
                                .env("DEBIAN_FRONTEND", "noninteractive"),
                        ]
                    };
                    self.busy_job = Some(item.name.clone());
                    self.busy_item = Some(item.id.clone());
                    self.last_cmd_log = format!(
                        "apt-get {} {deb_pkg}",
                        if want_install { "install" } else { "remove" }
                    );
                    self.status_banner = format!(
                        "{} {deb_pkg}…",
                        if want_install { "Installing" } else { "Removing" }
                    );
                    backend::spawn_command_job(
                        Some(item.id),
                        Some(want_install),
                        item.name,
                        steps,
                        true,
                        self.job_tx.clone(),
                    );
                    return;
                }
            }
        }

        // 4. Fallback / Linux patch simulation when offline or in dry-run tests
        if let Some(it) = self.items.get_mut(idx) {
            if it.update_available {
                it.update_available = false;
                it.installed = true;
                if let Some((_, new_ver)) = it.version.split_once("->") {
                    it.version = new_ver.trim().to_string();
                }
                self.last_cmd_log = cmd_preview;
                self.status_banner = format!("Updated {} ({})", it.name, it.version);
            } else if !it.installed {
                it.installed = true;
                self.last_cmd_log = cmd_preview;
                self.status_banner = format!("Installed {} via {}", it.name, it.source.badge());
            } else if it.source != PackageSource::LinuxPatch {
                it.installed = false;
                self.last_cmd_log = cmd_preview;
                self.status_banner = format!("Removed {}", it.name);
            } else {
                self.status_banner = format!("{} is active in the running kernel", it.name);
            }
        }
        self.save_state();
    }

    // --- Chrome geometry (top → bottom: TopAppBar, SearchPill, SegmentedBar) ---

    fn top_bar_rect(&self) -> Rect {
        widgets::TopAppBar::layout(self.size, ui::TOP_SAFE)
    }

    /// Log action: a 48 px icon button in the app bar (audit S11 — the old
    /// "Log" pill floated over the header subtitle).
    fn log_action_rect(&self) -> Rect {
        widgets::TopAppBar::action_rect(self.top_bar_rect())
    }

    fn search_bar_rect(&self) -> Rect {
        let w = self.size.w.max(360);
        Rect::new(
            ui::MARGIN,
            self.top_bar_rect().bottom() + 12,
            w - ui::MARGIN * 2,
            widgets::SearchPill::HEIGHT,
        )
    }

    fn tab_bar_rect(&self) -> Rect {
        let w = self.size.w.max(360);
        Rect::new(
            ui::MARGIN,
            self.search_bar_rect().bottom() + 12,
            w - ui::MARGIN * 2,
            48,
        )
    }

    /// Per-segment rects; mirrors `widgets::segmented`'s even split exactly
    /// so the drawn segments and hit targets stay single-sourced.
    fn tab_rects(&self) -> [(StoreTab, Rect); 4] {
        let bar = self.tab_bar_rect();
        let mut out = [(StoreTab::Updates, Rect::new(0, 0, 0, 0)); 4];
        for (i, tab) in StoreTab::ALL.iter().enumerate() {
            let x0 = bar.x + (bar.w as i64 * i as i64 / 4) as i32;
            let x1 = bar.x + (bar.w as i64 * (i as i64 + 1) / 4) as i32;
            out[i] = (*tab, Rect::new(x0, bar.y, (x1 - x0).max(1), bar.h));
        }
        out
    }

    fn list_top(&self) -> i32 {
        self.tab_bar_rect().bottom() + 12
    }

    /// Clip band the scrollable list is allowed to occupy.
    fn viewport_rect(&self) -> Rect {
        Rect::new(0, self.list_top(), self.size.w, self.size.h - self.list_top())
    }

    // --- Scrollable-list geometry (all shifted by scroll_y) ---

    fn status_strip_rect(&self) -> Rect {
        let w = self.size.w.max(360);
        Rect::new(
            ui::MARGIN,
            self.list_top() - self.scroll_y,
            w - ui::MARGIN * 2,
            STRIP_H,
        )
    }

    fn hero_card_rect(&self) -> Rect {
        let w = self.size.w.max(360);
        Rect::new(
            ui::MARGIN,
            self.list_top() + STRIP_H - self.scroll_y,
            w - ui::MARGIN * 2,
            HERO_H,
        )
    }

    fn hero_button_rects(&self) -> (Rect, Rect) {
        let hero = self.hero_card_rect();
        let btn_w = (hero.w - 44) / 2;
        let y = hero.bottom() - 62;
        let check_r = Rect::new(hero.x + 16, y, btn_w, 48);
        let apply_r = Rect::new(hero.x + 28 + btn_w, y, btn_w, 48);
        (check_r, apply_r)
    }

    /// `(item_idx, row_rect, action_rect)` — `action_rect` is empty when the
    /// item has no direct action (installed rows are only informational;
    /// removal lives on the detail page behind a confirm dialog — the fix
    /// for the one-tap destructive uninstall).
    fn item_card_rects(&self) -> Vec<(usize, Rect, Rect)> {
        let w = self.size.w.max(360);
        let mut y = self.list_top() + STRIP_H - self.scroll_y;
        if self.tab == StoreTab::Updates {
            y += HERO_H + HERO_GAP;
        }
        let row_w = w - ui::MARGIN * 2;
        let mut out = Vec::new();
        for idx in self.visible_indices() {
            let row = Rect::new(ui::MARGIN, y, row_w, ROW_H);
            let item = &self.items[idx];
            let btn = if item.update_available || !item.installed {
                Rect::new(
                    row.right() - ROW_ACTION_W,
                    row.y + (ROW_H - 48) / 2,
                    ROW_ACTION_W,
                    48,
                )
            } else {
                Rect::new(0, 0, 0, 0)
            };
            out.push((idx, row, btn));
            y += ROW_H;
        }
        out
    }

    // --- Log bottom-sheet geometry ---

    fn log_sheet_rect(&self) -> Rect {
        let h = (self.size.h * 3 / 5).clamp(400, self.size.h - ui::TOP_SAFE - 40);
        widgets::BottomSheet::sheet_rect(self.size, h)
    }

    /// `(clear, close)` — both ≥48 px icon buttons on the sheet title row.
    fn log_sheet_buttons(&self) -> (Rect, Rect) {
        let s = self.log_sheet_rect();
        let close_r = Rect::new(s.right() - ui::MARGIN - 48, s.y + 16, 48, 48);
        let clear_r = Rect::new(close_r.x - 8 - 48, s.y + 16, 48, 48);
        (clear_r, close_r)
    }

    // --- Detail-page geometry (single-sourced like the list trio) ---

    fn detail_tile_rect(&self) -> Rect {
        Rect::new(ui::MARGIN, self.top_bar_rect().bottom() + 24, 96, 96)
    }

    fn detail_action_rect(&self) -> Rect {
        let tile = self.detail_tile_rect();
        Rect::new(ui::MARGIN, tile.bottom() + 20, self.size.w - ui::MARGIN * 2, 56)
    }

    fn detail_stats_rect(&self) -> Rect {
        let action = self.detail_action_rect();
        Rect::new(action.x, action.bottom() + 20, action.w, 80)
    }

    /// Whether the detail page's action button is live. Installed Linux
    /// patch items are system components — they can't be removed.
    fn detail_action_enabled(&self, idx: usize) -> bool {
        let Some(it) = self.items.get(idx) else {
            return false;
        };
        it.update_available || !it.installed || it.source != PackageSource::LinuxPatch
    }

    /// Title/body/layout of the uninstall confirm dialog — one builder so
    /// `hit_test` and `render` can't disagree (M3 destructive-action confirm).
    fn uninstall_dialog(&self, idx: usize) -> (String, Vec<String>, widgets::DialogLayout) {
        let name = self
            .items
            .get(idx)
            .map_or("this app".to_string(), |i| i.name.clone());
        let title = format!("Uninstall {name}?");
        let body = match self.items.get(idx) {
            Some(item) => vec![
                format!("{} will be removed from this device.", item.name),
                format!("{} · {}", item.source.badge(), item.size_label),
            ],
            None => Vec::new(),
        };
        let refs: Vec<&str> = body.iter().map(String::as_str).collect();
        let layout = widgets::dialog_layout(self.size, &refs, &["Cancel", "Uninstall"]);
        (title, body, layout)
    }

    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }

    fn reset_scroll(&mut self) {
        self.scroll.scroll_to(0.0);
        self.scroll_y = 0;
    }

    fn hit_test(&self, pos: Point) -> Hit {
        // Modal uninstall confirm swallows everything else first.
        if let Some(idx) = self.confirm_uninstall {
            let (_, _, layout) = self.uninstall_dialog(idx);
            if let Some(i) = widgets::dialog_action_at(&layout, pos) {
                return Hit::DialogAction(i);
            }
            if widgets::dialog_on_scrim(&layout, self.size, pos) {
                return Hit::DialogDismiss;
            }
            return Hit::None;
        }
        // Modal log sheet: buttons + outside-tap close.
        if self.show_log {
            let (clear_r, close_r) = self.log_sheet_buttons();
            if clear_r.contains(pos) {
                return Hit::ClearLog;
            }
            if close_r.contains(pos) {
                return Hit::CloseLog;
            }
            if widgets::BottomSheet::hit_outside(self.size, self.log_sheet_rect(), pos) {
                return Hit::CloseLog;
            }
            return Hit::None;
        }
        // Detail page: back arrow + full-width action.
        if let Some(idx) = self.detail {
            if widgets::TopAppBar::hit(self.top_bar_rect(), pos, true, false)
                == Some(widgets::TopBarHit::Back)
            {
                return Hit::DetailBack;
            }
            if self.detail_action_rect().contains(pos) && self.detail_action_enabled(idx) {
                return Hit::DetailAction(idx);
            }
            return Hit::None;
        }
        // Main list chrome.
        if self.log_action_rect().contains(pos) {
            return Hit::ToggleLog;
        }
        match widgets::SearchPill::hit(
            self.search_bar_rect(),
            pos,
            !self.search.is_empty(),
        ) {
            Some(widgets::SearchHit::Clear) => return Hit::ClearSearch,
            Some(widgets::SearchHit::Field) => return Hit::SearchBar,
            None => {}
        }
        for (tab, rect) in self.tab_rects() {
            if rect.contains(pos) {
                return Hit::Tab(tab);
            }
        }
        if pos.y < self.list_top() {
            return Hit::None;
        }
        if self.tab == StoreTab::Updates {
            let (check_r, apply_r) = self.hero_button_rects();
            if check_r.contains(pos) {
                return Hit::CheckUpdatesBtn;
            }
            // Disabled (no live hit) when there is nothing to apply (S13).
            if apply_r.contains(pos) && self.pending_updates_count() > 0 {
                return Hit::ApplyAllUpdatesBtn;
            }
        }
        for (idx, row, btn) in self.item_card_rects() {
            if !btn.is_empty() && btn.contains(pos) {
                return Hit::ItemAction(idx);
            }
            if row.contains(pos) {
                return Hit::ItemOpen(idx);
            }
        }
        Hit::None
    }

    /// Re-clamp the `ScrollView` to the current content/viewport bounds and
    /// mirror the offset into `scroll_y` (kept for the geometry contract).
    fn sync_scroll(&mut self) {
        let viewport_h = (self.size.h - self.list_top()).max(100);
        self.scroll
            .set_bounds(self.content_h as f32, viewport_h as f32);
        self.scroll_y = self.scroll.offset_i32();
    }

    // --- Drawing helpers (list is painted into an offscreen layer that is
    // clipped to the scroll viewport by `blit_region`) ---

    /// Fixed chrome: app bar (title + log icon), SearchPill, segmented tabs,
    /// and the scrollbar indicator.
    fn render_chrome(&self, canvas: &mut Canvas, theme: &Theme) {
        widgets::TopAppBar::render(
            canvas,
            self.top_bar_rect(),
            theme,
            "Store & Updates",
            false,
            None,
        );
        let lr = self.log_action_rect();
        widgets::icon_btn(
            canvas,
            theme,
            lr,
            ICON_TERMINAL,
            widgets::IconBtnStyle::Plain,
            self.show_log,
            self.pressed == Hit::ToggleLog,
        );
        if self.busy_job.is_some() {
            let dot = Rect::new(lr.right() - 14, lr.y + 8, 10, 10);
            canvas.fill_rounded_rect(dot, 5.0, theme.accent);
        }

        widgets::SearchPill::render(
            canvas,
            self.search_bar_rect(),
            theme,
            self.search.as_str(),
            "Search apps",
            self.search_focused,
        );

        let pending = self.pending_updates_count();
        let labels = [
            if pending > 0 {
                format!("Updates ({pending})")
            } else {
                "Updates".to_string()
            },
            "Featured".to_string(),
            "Flatpak".to_string(),
            "Paru AUR".to_string(),
        ];
        let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let active = StoreTab::ALL
            .iter()
            .position(|t| *t == self.tab)
            .unwrap_or(0);
        widgets::segmented(canvas, theme, self.tab_bar_rect(), &label_refs, active);
        if let Hit::Tab(t) = self.pressed {
            for (tab, r) in self.tab_rects() {
                if tab == t {
                    ui::press_overlay(canvas, theme, r, r.h as f32 / 2.0, 0.10);
                }
            }
        }

        let vp = self.viewport_rect();
        ui::scrollbar(
            canvas,
            theme,
            Rect::new(self.size.w - 9, vp.y + 8, 6, vp.h - 16),
            self.scroll_y,
            self.content_h,
        );
    }

    /// Everything that lives in the scroll viewport: status strip, Updates
    /// hero card, item rows, or the empty state.
    fn render_list(&self, c: &mut Canvas, theme: &Theme) {
        let list_top = self.list_top();

        let strip = self.status_strip_rect();
        let (sicon, scol) = if self.busy_job.is_some() {
            (icons::REFRESH, theme.accent)
        } else if self.status_banner.to_ascii_lowercase().contains("fail") {
            (icons::WARNING, theme.danger)
        } else {
            (icons::CHECK_CIRCLE, theme.text3)
        };
        ui::draw_icon(c, sicon, 20.0, scol, strip.x + 2, strip.y + (STRIP_H - 20) / 2);
        ui::draw_text(
            c,
            &self.status_banner,
            &ui::caption(theme.text2),
            strip.x + 30,
            strip.y + (STRIP_H - 22) / 2,
            Some(strip.w - 34),
        );

        if self.tab == StoreTab::Updates {
            self.render_hero(c, theme);
        }

        let rows = self.item_card_rects();
        if rows.is_empty() {
            self.render_empty(c, theme);
            return;
        }
        for (idx, row, btn) in rows {
            if row.bottom() < list_top || row.y > self.size.h {
                continue;
            }
            self.render_row(c, theme, idx, row, btn);
        }
    }

    /// Updates-tab hero: pending-count headline + kernel/channel + the two
    /// update actions. The raw `$ {last_cmd_log}` developer line is gone
    /// (audit S13) and "Update All" disables when nothing is pending.
    fn render_hero(&self, c: &mut Canvas, theme: &Theme) {
        let hero = self.hero_card_rect();
        if hero.bottom() <= self.list_top() {
            return;
        }
        let pending = self.pending_updates_count();
        c.fill_rounded_rect(hero, tokens::RADIUS_CARD, theme.surface1);
        let (icon, headline, head_col) = if pending > 0 {
            (
                icons::SYSTEM_UPDATE,
                format!("{pending} Linux & Driver Patches Available"),
                theme.accent,
            )
        } else {
            (
                icons::CHECK_CIRCLE,
                "RabbizOS Linux & Drivers Up to Date".to_string(),
                theme.success,
            )
        };
        ui::draw_icon(c, icon, 26.0, head_col, hero.x + 20, hero.y + 16);
        ui::draw_text(
            c,
            &headline,
            &ui::styled(22.0, 600, head_col),
            hero.x + 56,
            hero.y + 16,
            Some(hero.w - 76),
        );
        ui::draw_text(
            c,
            &format!(
                "Kernel: {} · Checked: {}",
                self.kernel_release, self.last_checked
            ),
            &ui::caption(theme.text2),
            hero.x + 20,
            hero.y + 54,
            Some(hero.w - 40),
        );
        ui::draw_text(
            c,
            &format!("Channel: {}", self.os_channel),
            &ui::caption(theme.text3),
            hero.x + 20,
            hero.y + 82,
            Some(hero.w - 40),
        );

        let (check_r, apply_r) = self.hero_button_rects();
        ui::draw_pill_button(
            c,
            theme,
            check_r,
            "Check Updates",
            false,
            self.pressed == Hit::CheckUpdatesBtn,
        );
        if pending > 0 {
            ui::draw_pill_button(
                c,
                theme,
                apply_r,
                "Update All Now",
                true,
                self.pressed == Hit::ApplyAllUpdatesBtn,
            );
        } else {
            c.fill_rounded_rect(apply_r, (apply_r.h as f32) / 2.0, theme.surface2);
            ui::draw_centered(c, "All Patched", &ui::label(theme.text3), apply_r);
        }
    }

    /// One M3 list row: tinted source-tile + name + `{source} · {size} ·
    /// {version}` caption + tonal Install/Update button, a busy "…" pill with
    /// indeterminate progress, or a non-live "Installed" check (S4/S7).
    fn render_row(&self, c: &mut Canvas, theme: &Theme, idx: usize, row: Rect, btn: Rect) {
        let item = &self.items[idx];
        let cy = row.y + row.h / 2;
        let tint = source_tint(item.source, theme);

        let tile = Rect::new(row.x, cy - ICON_TILE / 2, ICON_TILE, ICON_TILE);
        c.fill_rounded_rect(tile, 18.0, ui::with_alpha(tint, 0x26));
        ui::draw_icon(
            c,
            source_glyph(item.source),
            30.0,
            tint,
            tile.x + (ICON_TILE - 30) / 2,
            tile.y + (ICON_TILE - 30) / 2,
        );

        let busy = self.busy_item.as_deref() == Some(item.id.as_str());
        let installed_caption = (!busy && btn.is_empty()).then(|| {
            text::label("Installed", ui::caption(theme.text3), None)
                .map_or(64, |px| px.width() as i32)
        });
        let trailing_w = if busy || !btn.is_empty() {
            ROW_ACTION_W + 16
        } else {
            installed_caption.unwrap_or(0) + 48
        };
        let text_x = row.x + ICON_TILE + 20;
        let text_right = (row.right() - trailing_w).max(text_x + 40);
        draw_line(
            c,
            &item.name,
            &ui::body(theme.text),
            Rect::new(text_x, cy - 30, text_right - text_x, 30),
        );
        draw_line(
            c,
            &row_caption(item),
            &ui::caption(theme.text2),
            Rect::new(text_x, cy + 2, text_right - text_x, 26),
        );

        if busy {
            // Per-item progress: the action morphs into a status pill and an
            // indeterminate track under the text column (audit S7).
            let pill = Rect::new(row.right() - ROW_ACTION_W, cy - 24, ROW_ACTION_W, 48);
            c.fill_rounded_rect(pill, (pill.h as f32) / 2.0, theme.surface2);
            let verb = if item.update_available {
                "Updating…"
            } else if item.installed {
                "Removing…"
            } else {
                "Installing…"
            };
            ui::draw_centered(c, verb, &ui::label(theme.accent), pill);
            let track = Rect::new(text_x, row.bottom() - 10, text_right - text_x, 4);
            c.fill_rounded_rect(track, 2.0, theme.surface3);
            let seg_w = (track.w * 2 / 5).max(24);
            let span = (track.w + seg_w) as f32;
            let phase = (self.anim_t / 1.1).fract();
            let seg_x = track.x + (phase * span) as i32 - seg_w;
            if let Some(segment) = track.intersect(Rect::new(seg_x, track.y, seg_w, 4)) {
                c.fill_rounded_rect(segment, 2.0, theme.accent);
            }
        } else if !btn.is_empty() {
            let label = if item.update_available {
                "Update"
            } else {
                "Install"
            };
            c.fill_rounded_rect(btn, (btn.h as f32) / 2.0, theme.surface2);
            if self.pressed == Hit::ItemAction(idx) {
                ui::press_overlay(c, theme, btn, (btn.h as f32) / 2.0, 0.12);
            }
            ui::draw_centered(c, label, &ui::label(theme.accent), btn);
        } else if let Some(cap_w) = installed_caption {
            // Non-live status only — uninstall is on the detail page behind a
            // confirm dialog (destructive-action fix).
            ui::draw_icon(
                c,
                icons::CHECK,
                18.0,
                theme.success,
                row.right() - cap_w - 26,
                cy - 9,
            );
            ui::draw_text(
                c,
                "Installed",
                &ui::caption(theme.text3),
                row.right() - cap_w,
                cy - 12,
                Some(cap_w),
            );
        }

        ui::divider_inset(c, theme, text_x, row.right(), row.bottom() - 1);
        if self.pressed == Hit::ItemOpen(idx) || self.pressed == Hit::ItemAction(idx) {
            c.fill_rect(row, ui::state_layer(theme, 0.08));
        }
    }

    /// Empty tab / no-search-result placeholder (audit S14).
    fn render_empty(&self, c: &mut Canvas, theme: &Theme) {
        let mut y = self.list_top() + STRIP_H;
        if self.tab == StoreTab::Updates {
            y += HERO_H + HERO_GAP;
        }
        let area = Rect::new(
            0,
            y,
            self.size.w,
            (self.size.h - y - ui::BOTTOM_SAFE).max(0),
        );
        let q = self.search.as_str().trim();
        if q.is_empty() {
            widgets::EmptyState::render(
                c,
                area,
                theme,
                icons::APPS,
                "Nothing here yet",
                "This feed is empty right now.",
            );
        } else {
            widgets::EmptyState::render(
                c,
                area,
                theme,
                icons::SEARCH,
                "No results",
                &format!("No matches for \u{201c}{q}\u{201d} in this tab."),
            );
        }
    }

    /// App details: package identity, management action, and metadata.
    fn render_detail(&mut self, canvas: &mut Canvas, theme: &Theme, idx: usize) {
        let Some(item) = self.items.get(idx).cloned() else {
            self.detail = None;
            return;
        };
        widgets::TopAppBar::render(
            canvas,
            self.top_bar_rect(),
            theme,
            "App details",
            true,
            None,
        );

        let tint = source_tint(item.source, theme);
        let tile = self.detail_tile_rect();
        canvas.fill_rounded_rect(tile, 24.0, ui::with_alpha(tint, 0x26));
        ui::draw_icon(
            canvas,
            source_glyph(item.source),
            46.0,
            tint,
            tile.x + (tile.w - 46) / 2,
            tile.y + (tile.h - 46) / 2,
        );

        let tx = tile.right() + 24;
        let name_r = ui::draw_text(
            canvas,
            &item.name,
            &ui::styled(28.0, 600, theme.text),
            tx,
            tile.y + 6,
            Some(self.size.w - tx - ui::MARGIN),
        );
        ui::draw_text(
            canvas,
            item.source.badge(),
            &ui::styled(15.0, 600, tint),
            tx,
            (name_r.bottom() + 6).max(tile.y + 46),
            Some(self.size.w - tx - ui::MARGIN),
        );

        // Stats card: VERSION / SIZE / SOURCE columns with hairline splits.
        let stats = self.detail_stats_rect();
        canvas.fill_rounded_rect(stats, 20.0, theme.surface1);
        let cols = [
            ("VERSION", item.version.replace("->", " \u{2192} ")),
            ("SIZE", item.size_label.clone()),
            ("SOURCE", src_short(item.source).to_string()),
        ];
        let cw = stats.w / 3;
        for (i, (key, value)) in cols.iter().enumerate() {
            let cx = stats.x + cw * i as i32;
            if i > 0 {
                canvas.fill_rect(
                    Rect::new(cx, stats.y + 20, 1, stats.h - 40),
                    theme.line,
                );
            }
            ui::draw_centered(
                canvas,
                value,
                &ui::label(theme.text),
                Rect::new(cx + 8, stats.y + 14, cw - 16, 30),
            );
            ui::draw_centered(
                canvas,
                key,
                &ui::caption(theme.text3),
                Rect::new(cx + 8, stats.y + 50, cw - 16, 24),
            );
        }

        // Full-width action: Install / Update (accent), Uninstall (danger
        // label — confirms via dialog), or a disabled system-component note.
        let act = self.detail_action_rect();
        let enabled = self.detail_action_enabled(idx);
        let destructive = item.installed && !item.update_available;
        let (label, fill, fg) = if !enabled {
            ("System component — active", theme.surface2, theme.text3)
        } else if destructive {
            ("Uninstall", theme.surface2, theme.danger)
        } else if item.update_available {
            ("Update", theme.accent, theme.text_on_accent)
        } else {
            ("Install", theme.accent, theme.text_on_accent)
        };
        canvas.fill_rounded_rect(act, (act.h as f32) / 2.0, fill);
        if enabled && self.pressed == Hit::DetailAction(idx) {
            ui::press_overlay(canvas, theme, act, (act.h as f32) / 2.0, 0.12);
        }
        ui::draw_centered(canvas, label, &ui::label(fg), act);

        let about_y = stats.bottom() + 20;
        ui::draw_text(
            canvas,
            "About this app",
            &ui::styled(20.0, 600, theme.text),
            ui::MARGIN,
            about_y,
            Some(self.size.w - ui::MARGIN * 2),
        );
        ui::draw_text(
            canvas,
            &item.summary,
            &ui::body(theme.text2),
            ui::MARGIN,
            about_y + 36,
            Some(self.size.w - ui::MARGIN * 2),
        );
    }

    /// Log viewer as a proper M3 bottom sheet: scrim + drag handle + surface1
    /// sheet (kit `BottomSheet`), icon-only ≥48 px Clear/Close, and a console
    /// panel whose colors all derive from theme tokens (audit S1/S12 — the
    /// old sheet hardcoded four RGBA constants).
    fn render_log_sheet(&self, canvas: &mut Canvas, theme: &Theme) {
        let sheet = self.log_sheet_rect();
        let title = if let Some(job) = &self.busy_job {
            format!("Running: {job}")
        } else {
            "Activity log".to_string()
        };
        widgets::BottomSheet::render_backdrop_and_sheet(
            canvas,
            self.size,
            sheet,
            theme,
            &title,
        );
        let (clear_r, close_r) = self.log_sheet_buttons();
        widgets::icon_btn(
            canvas,
            theme,
            clear_r,
            icons::DELETE,
            widgets::IconBtnStyle::Tonal,
            false,
            self.pressed == Hit::ClearLog,
        );
        widgets::icon_btn(
            canvas,
            theme,
            close_r,
            icons::CLOSE,
            widgets::IconBtnStyle::Plain,
            false,
            self.pressed == Hit::CloseLog,
        );

        // `icon_glyph` is plum in both themes → an 88 % mix stays a dark
        // console on cream and plum palettes alike.
        let console_bg = ui::mix(theme.canvas, theme.icon_glyph, 0.88);
        let panel = Rect::new(
            sheet.x + 16,
            sheet.y + 80,
            sheet.w - 32,
            sheet.h - 80 - ui::BOTTOM_SAFE - 12,
        );
        canvas.fill_rounded_rect(panel, 20.0, console_bg);

        let line_h = 24;
        let body_top = panel.y + 12;
        let max_lines = ((panel.h - 24) / line_h).max(1) as usize;
        let skip = self.log_lines.len().saturating_sub(max_lines);
        for (row, line) in self.log_lines.iter().skip(skip).enumerate() {
            let col = if line.starts_with("$ ") {
                theme.accent_hi
            } else if line.starts_with("[done]") {
                theme.success
            } else if line.starts_with("[failed]")
                || line.starts_with("error:")
                || line.contains("Err")
            {
                theme.danger
            } else {
                theme.icon_tile
            };
            ui::draw_text(
                canvas,
                line,
                &ui::styled(15.0, 400, col),
                panel.x + 16,
                body_top + row as i32 * line_h,
                Some(panel.w - 32),
            );
        }
    }

    /// M3 destructive-action confirm (kit `dialog`) — the uninstall fix.
    fn render_uninstall_dialog(&self, canvas: &mut Canvas, theme: &Theme, idx: usize) {
        let (title, body, _) = self.uninstall_dialog(idx);
        let refs: Vec<&str> = body.iter().map(String::as_str).collect();
        let pressed_idx = match self.pressed {
            Hit::DialogAction(i) => Some(i),
            _ => None,
        };
        widgets::dialog(
            canvas,
            theme,
            self.size,
            &title,
            &refs,
            &["Cancel", "Uninstall"],
            pressed_idx,
        );
    }
}

/// Material glyph per package source (audit S4 — icon tile per source).
fn source_glyph(s: PackageSource) -> &'static str {
    match s {
        PackageSource::LinuxPatch => icons::SYSTEM_UPDATE,
        PackageSource::Recommended => icons::STAR,
        PackageSource::Flatpak => icons::DOWNLOAD,
        PackageSource::Paru => ICON_BUILD,
    }
}

/// Per-source tint for the icon tile (accent for system patches; the kit's
/// avatar palette for store feeds — all kit-provided colors).
fn source_tint(s: PackageSource, t: &Theme) -> [u8; 4] {
    match s {
        PackageSource::LinuxPatch => t.accent,
        PackageSource::Recommended => ui::AVATAR_TINTS[1],
        PackageSource::Flatpak => ui::AVATAR_TINTS[0],
        PackageSource::Paru => ui::AVATAR_TINTS[4],
    }
}

/// Short source label for row captions / stats ("Flatpak" not "FLATPAK · ARM64").
fn src_short(s: PackageSource) -> &'static str {
    match s {
        PackageSource::LinuxPatch => "Linux patch",
        PackageSource::Recommended => "Curated",
        PackageSource::Flatpak => "Flatpak",
        PackageSource::Paru => "AUR",
    }
}

/// `{source} · {size} · {version}` caption — package ids and the raw
/// `v{ver} · {id}` junk are gone; `a -> b` collapses to `a → b` (audit S4).
fn row_caption(item: &CatalogItem) -> String {
    format!(
        "{} · {} · {}",
        src_short(item.source),
        item.size_label,
        item.version.replace("->", " -> ")
    )
}

/// Single-line ellipsized label vertically centered in `r` — mirrors the
/// kit's private `draw_single_line_vcenter` (kit gap: not exported).
fn draw_line(c: &mut Canvas, s: &str, st: &text::LabelStyle, r: Rect) {
    if s.is_empty() || r.w <= 0 || r.h <= 0 {
        return;
    }
    if let Some(px) = text::label_clamped(s, *st, r.w.max(8), 1) {
        let w = px.width() as i32;
        let h = px.height() as i32;
        c.blit(
            &de_api::Pixels::new(Size::new(w, h), px.data().to_vec()),
            Point::new(r.x, r.y + (r.h - h).max(0) / 2),
        );
    }
}

impl AppState for StoreApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-store"
    }

    fn title(&self) -> &'static str {
        "Store & Updates"
    }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        let job_changed = self.drain_jobs();
        match event {
            AppEvent::TouchDown { id, pos } => {
                self.pressed = self.hit_test(pos);
                // Scroll drags only start inside the list viewport on the main
                // screen (not over modals, chrome, or the detail page).
                self.scroll_touch = None;
                if self.confirm_uninstall.is_none()
                    && !self.show_log
                    && self.detail.is_none()
                    && pos.y >= self.list_top()
                {
                    self.scroll.on_touch_down(pos.y as f32, self.now_ms());
                    self.scroll_touch = Some(id);
                }
                AppFrame {
                    dirty: true,
                    ..Default::default()
                }
            }
            AppEvent::TouchMove { id, pos } => {
                if self.scroll_touch != Some(id) {
                    return AppFrame::default();
                }
                if self.scroll.on_touch_move(pos.y as f32, self.now_ms()) {
                    self.pressed = Hit::None;
                    self.scroll_y = self.scroll.offset_i32();
                    return AppFrame {
                        dirty: true,
                        ..Default::default()
                    };
                }
                AppFrame::default()
            }
            AppEvent::TouchUp { id, pos } => {
                let was_drag = if self.scroll_touch == Some(id) {
                    self.scroll_touch = None;
                    self.scroll.on_touch_up(self.now_ms())
                } else {
                    false
                };
                let hit = self.pressed;
                self.pressed = Hit::None;
                if !was_drag && hit == self.hit_test(pos) {
                    match hit {
                        Hit::Tab(t) => {
                            self.tab = t;
                            self.reset_scroll();
                            self.search_focused = false;
                            ti.deactivate();
                        }
                        Hit::SearchBar => {
                            self.search_focused = true;
                            ti.activate(wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose::Normal);
                        }
                        Hit::ClearSearch => {
                            self.search.clear();
                            self.reset_scroll();
                        }
                        Hit::ToggleLog => {
                            self.show_log = !self.show_log;
                        }
                        Hit::ClearLog => {
                            self.log_lines.clear();
                            self.push_log_line("Log cleared.".into());
                        }
                        Hit::CloseLog => {
                            self.show_log = false;
                        }
                        Hit::CheckUpdatesBtn => {
                            self.show_log = true;
                            self.check_for_updates();
                        }
                        Hit::ApplyAllUpdatesBtn => {
                            self.show_log = true;
                            self.apply_all_updates();
                        }
                        Hit::ItemAction(idx) => {
                            // Row buttons are never destructive (install /
                            // update only); a busy slot means a job is in
                            // flight for this item already.
                            if self.busy_job.is_none() {
                                if !backend::dry_run() {
                                    self.show_log = true;
                                }
                                self.trigger_item_action(idx);
                            }
                        }
                        Hit::ItemOpen(idx) => {
                            self.detail = Some(idx);
                            self.search_focused = false;
                            ti.deactivate();
                        }
                        Hit::DetailBack => {
                            self.detail = None;
                        }
                        Hit::DetailAction(idx) => {
                            if self.busy_job.is_none() {
                                let destructive = self.items.get(idx).is_some_and(|i| {
                                    i.installed && !i.update_available
                                });
                                if destructive {
                                    // MANDATORY: destructive uninstall goes
                                    // through a confirm dialog first.
                                    self.confirm_uninstall = Some(idx);
                                } else {
                                    if !backend::dry_run() {
                                        self.show_log = true;
                                    }
                                    self.trigger_item_action(idx);
                                }
                            }
                        }
                        Hit::DialogAction(1) => {
                            if let Some(idx) = self.confirm_uninstall.take() {
                                if !backend::dry_run() {
                                    self.show_log = true;
                                }
                                self.trigger_item_action(idx);
                            }
                        }
                        Hit::DialogAction(_) | Hit::DialogDismiss => {
                            self.confirm_uninstall = None;
                        }
                        Hit::None => {
                            if self.search_focused {
                                self.search_focused = false;
                                ti.deactivate();
                            }
                        }
                    }
                }
                AppFrame {
                    dirty: true,
                    ..Default::default()
                }
            }
            AppEvent::Scroll { dy, .. } => {
                // Wheel/trackpad: positive dy scrolls content down per the kit
                // contract → the scroll offset decreases.
                if self.detail.is_none()
                    && !self.show_log
                    && self.confirm_uninstall.is_none()
                {
                    self.scroll.scroll_by(-dy);
                    self.scroll_y = self.scroll.offset_i32();
                    return AppFrame {
                        dirty: true,
                        ..Default::default()
                    };
                }
                AppFrame::default()
            }
            AppEvent::Key { keysym: 0xff1b, .. } => {
                // Esc unwinds one level: dialog → sheet → detail → IME.
                if self.confirm_uninstall.is_some() {
                    self.confirm_uninstall = None;
                } else if self.show_log {
                    self.show_log = false;
                } else if self.detail.is_some() {
                    self.detail = None;
                } else if self.search_focused {
                    self.search_focused = false;
                    ti.deactivate();
                } else {
                    return AppFrame::default();
                }
                AppFrame {
                    dirty: true,
                    ..Default::default()
                }
            }
            AppEvent::Key { keysym: 0xff08, .. } => {
                if self.search_focused && self.search.backspace() {
                    self.reset_scroll();
                    return AppFrame {
                        dirty: true,
                        ..Default::default()
                    };
                }
                AppFrame::default()
            }
            AppEvent::Key {
                utf8: Some(text), ..
            } => {
                if self.search_focused && !text.chars().any(char::is_control) {
                    self.search.insert(&text);
                    self.reset_scroll();
                    return AppFrame {
                        dirty: true,
                        ..Default::default()
                    };
                }
                AppFrame::default()
            }
            AppEvent::TextCommit(text) => {
                if self.search_focused {
                    self.search.insert(&text);
                    self.reset_scroll();
                    return AppFrame {
                        dirty: true,
                        ..Default::default()
                    };
                }
                AppFrame::default()
            }
            AppEvent::TextBackspace(n) => {
                if self.search_focused {
                    for _ in 0..n {
                        self.search.backspace();
                    }
                    self.reset_scroll();
                    return AppFrame {
                        dirty: true,
                        ..Default::default()
                    };
                }
                AppFrame::default()
            }
            AppEvent::Tick(dt) => {
                if self.busy_job.is_some() {
                    self.anim_t += dt.as_secs_f32();
                }
                let settling = self.scroll.tick(dt.as_secs_f32());
                self.scroll_y = self.scroll.offset_i32();
                AppFrame {
                    dirty: job_changed || settling || self.busy_job.is_some(),
                    animating: self.busy_job.is_some() || self.scroll.is_animating(),
                    quit: false,
                }
            }
            _ => AppFrame {
                dirty: job_changed,
                animating: self.busy_job.is_some() || self.scroll.is_animating(),
                quit: false,
            },
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        self.drain_jobs();
        self.size = canvas.size();
        canvas.clear(theme.bg);

        // Content height is derived from the same constants the geometry
        // fns use — no duplicated card_h/gap literals (audit S8).
        let count = self.visible_indices().len() as i32;
        self.content_h = STRIP_H
            + if self.tab == StoreTab::Updates {
                HERO_H + HERO_GAP
            } else {
                0
            }
            + count * ROW_H
            + ui::BOTTOM_SAFE
            + 24;
        self.sync_scroll();

        if let Some(idx) = self.detail {
            self.render_detail(canvas, theme, idx);
        } else {
            // Scrollable list is painted into an offscreen layer and blitted
            // through a strict clip band below the chrome — the old code
            // overpainted the header zone with fill_rect, which clipped card
            // corners mid-radius (audit S9).
            let mut layer = Canvas::new(canvas.size());
            layer.clear(theme.bg);
            self.render_list(&mut layer, theme);
            let vp = self.viewport_rect();
            canvas.blit_region(&layer.into_pixels(), vp, Point::new(vp.x, vp.y));

            self.render_chrome(canvas, theme);
        }

        if self.show_log {
            self.render_log_sheet(canvas, theme);
        }
        if let Some(idx) = self.confirm_uninstall {
            self.render_uninstall_dialog(canvas, theme, idx);
        }
    }
}

// ---------------------------------------------------------------------------
// Headless CLI (`--cli`) & AI Agent MCP (`--mcp`)
// ---------------------------------------------------------------------------

fn run_cli(args: &[String]) -> i32 {
    let mut app = StoreApp::new();
    let cmd = args.first().map_or("status", String::as_str);
    match cmd {
        "status" => {
            println!(
                "{}",
                serde_json::json!({
                    "kernel_release": app.kernel_release,
                    "os_channel": app.os_channel,
                    "pending_updates": app.pending_updates_count(),
                    "total_catalog_items": app.items.len(),
                })
            );
            0
        }
        "check" => {
            app.check_for_updates();
            println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "pending_updates": app.pending_updates_count(),
                    "status": app.status_banner,
                })
            );
            0
        }
        "update-all" => {
            let applied = app.apply_all_updates();
            println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "applied": applied,
                    "kernel_release": app.kernel_release,
                    "status": app.status_banner,
                })
            );
            0
        }
        "list" => {
            if let Some(tab_arg) = args.get(1) {
                if let Some(t) = StoreTab::parse(tab_arg) {
                    app.tab = t;
                }
            }
            let items: Vec<&CatalogItem> = app
                .visible_indices()
                .into_iter()
                .map(|i| &app.items[i])
                .collect();
            println!("{}", serde_json::to_string_pretty(&items).unwrap_or_default());
            0
        }
        "install" => {
            let Some(pkg_id) = args.get(1) else {
                eprintln!("usage: rabbiz-store --cli install <pkg_id>");
                return 1;
            };
            let Some(idx) = app.items.iter().position(|i| i.id == *pkg_id) else {
                eprintln!("package not found: {pkg_id}");
                return 1;
            };
            app.trigger_item_action(idx);
            println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "id": app.items[idx].id,
                    "installed": app.items[idx].installed,
                    "cmd": app.last_cmd_log,
                })
            );
            0
        }
        other => {
            eprintln!("unknown command {other:?} (expected status|check|update-all|list|install)");
            1
        }
    }
}

pub static MCP_TOOLS: &[Tool] = &[
    Tool {
        name: "store_status",
        description: "Get current RabbizOS Linux kernel version, pending patch count, and store catalog summary.",
        input_schema: r#"{"type":"object","properties":{}}"#,
        handler: |_args| {
            let app = StoreApp::new();
            Ok(serde_json::json!({
                "kernel_release": app.kernel_release,
                "os_channel": app.os_channel,
                "pending_updates": app.pending_updates_count(),
                "total_items": app.items.len(),
            })
            .to_string())
        },
    },
    Tool {
        name: "store_update_all",
        description: "Apply all pending Linux kernel, DTB, and driver patches in RabbizOS Store & Updater.",
        input_schema: r#"{"type":"object","properties":{}}"#,
        handler: |_args| {
            let mut app = StoreApp::new();
            let applied = app.apply_all_updates();
            Ok(serde_json::json!({
                "ok": true,
                "applied": applied,
                "kernel_release": app.kernel_release,
                "status": app.status_banner,
            })
            .to_string())
        },
    },
    Tool {
        name: "store_install",
        description: "Install or update a package by ID from Recommended, Flatpak, Paru AUR, or Linux Patches.",
        input_schema: r#"{"type":"object","required":["id"],"properties":{"id":{"type":"string"}}}"#,
        handler: |args| {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing id".to_string())?;
            let mut app = StoreApp::new();
            let idx = app
                .items
                .iter()
                .position(|i| i.id == id)
                .ok_or_else(|| format!("package not found: {id}"))?;
            app.trigger_item_action(idx);
            Ok(serde_json::json!({
                "ok": true,
                "id": app.items[idx].id,
                "installed": app.items[idx].installed,
                "cmd": app.last_cmd_log,
                "status": app.status_banner,
            })
            .to_string())
        },
    },
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(pos) = args.iter().position(|a| a == "--cli") {
        std::process::exit(run_cli(&args[pos + 1..]));
    }
    if args.iter().any(|a| a == "--mcp") {
        rabbiz_app_kit::mcp::serve(MCP_TOOLS, "rabbiz-store", "0.1.0");
    }
    rabbiz_app_kit::run_app(StoreApp::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_tabs_filter_linux_patches_recommended_flatpak_and_paru() {
        let tmp = std::env::temp_dir().join(format!("rabbiz-store-test-{}", std::process::id()));

        let mut app = StoreApp::with_state_path(tmp.join("state.json"));
        assert_eq!(app.app_id(), "rabbiz-store");
        assert_eq!(app.pending_updates_count(), 5);

        app.tab = StoreTab::Updates;
        assert_eq!(app.visible_indices().len(), 5);

        app.tab = StoreTab::Recommended;
        assert_eq!(app.visible_indices().len(), 5);

        app.tab = StoreTab::Flatpak;
        assert_eq!(app.visible_indices().len(), 5);

        app.tab = StoreTab::Paru;
        assert_eq!(app.visible_indices().len(), 5);

        // Apply all 5 Linux & driver updates
        let applied = app.apply_all_updates();
        assert_eq!(applied, 5);
        assert_eq!(app.pending_updates_count(), 0);
        let reopened = StoreApp::with_state_path(tmp.join("state.json"));
        assert_eq!(reopened.pending_updates_count(), 0);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Evidence screenshots (GLOBAL.md contract): drives the restyled app
    /// through its main states and writes PNGs to /tmp/rabbiz-shots.
    #[test]
    fn store_ui_screenshots() {
        let tmp = std::env::temp_dir().join(format!("rabbiz-store-after-{}", std::process::id()));
        let dir = std::path::Path::new("/tmp/rabbiz-shots");
        let center = |r: Rect| Point::new(r.x + r.w / 2, r.y + r.h / 2);

        let app = StoreApp::with_state_path(tmp.join("state.json"));
        let mut h = rabbiz_app_kit::TestHarness::new(app);
        h.save_png(&dir.join("store-after-updates.png")).unwrap();

        // Log bottom sheet via the header icon action
        let lr = h.state().log_action_rect();
        h.tap(center(lr));
        assert!(h.state().show_log);
        h.save_png(&dir.join("store-after-log.png")).unwrap();
        // outside-tap closes the modal sheet
        h.tap(Point::new(360, 200));
        assert!(!h.state().show_log);

        // Flatpak tab rows
        let tr = h.state().tab_rects()[2].1;
        h.tap(center(tr));
        assert_eq!(h.state().tab, StoreTab::Flatpak);
        h.save_png(&dir.join("store-after-flatpak.png")).unwrap();

        // Busy per-item progress row
        h.state_mut().busy_job = Some("VLC Media Player".into());
        h.state_mut().busy_item = Some("org.videolan.VLC".into());
        h.save_png(&dir.join("store-after-busy.png")).unwrap();
        h.state_mut().busy_job = None;
        h.state_mut().busy_item = None;

        // Search with no matches -> EmptyState
        let sb = h.state().search_bar_rect();
        h.tap(center(sb));
        h.type_text("zzz-nothing");
        h.save_png(&dir.join("store-after-empty-search.png")).unwrap();
        // search clear icon restores the list
        let clr = widgets::SearchPill::clear_rect(h.state().search_bar_rect());
        h.tap(center(clr));
        assert!(h.state().search.is_empty());

        // Row tap opens the detail page
        let (_, row, _) = h.state().item_card_rects()[0];
        h.tap(center(row));
        assert!(h.state().detail.is_some());
        h.save_png(&dir.join("store-after-detail.png")).unwrap();

        // Recommended tab first item is installed -> Uninstall needs confirm
        h.send(AppEvent::Key {
            keysym: 0xff1b,
            utf8: None,
        }); // Esc -> back to list
        assert!(h.state().detail.is_none());
        let rec_tab = h.state().tab_rects()[1].1;
        h.tap(center(rec_tab));
        let (idx, row, _) = h.state().item_card_rects()[0];
        h.tap(center(row));
        assert_eq!(h.state().detail, Some(idx));
        let act = h.state().detail_action_rect();
        h.tap(center(act));
        assert!(h.state().confirm_uninstall.is_some());
        assert!(h.state().items[idx].installed); // nothing removed yet
        h.save_png(&dir.join("store-after-uninstall-confirm.png")).unwrap();
        // Confirm -> actually removes (dry-run fallback path)
        let (_, _, layout) = h.state().uninstall_dialog(idx);
        h.tap(center(layout.actions[1]));
        assert!(h.state().confirm_uninstall.is_none());
        assert!(!h.state().items[idx].installed);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_harness_renders_and_switches_store_tabs() {
        let tmp = std::env::temp_dir().join(format!("rabbiz-store-ui-test-{}", std::process::id()));

        let app = StoreApp::with_state_path(tmp.join("state.json"));
        let mut h = rabbiz_app_kit::TestHarness::new(app);
        let px0 = h.render();
        assert_eq!(px0.size, Size::new(720, 1600));

        // Tap the "Flatpak" tab (3rd tab)
        let flatpak_tab_rect = h.state().tab_rects()[2].1;
        h.tap(Point::new(
            flatpak_tab_rect.x + flatpak_tab_rect.w / 2,
            flatpak_tab_rect.y + flatpak_tab_rect.h / 2,
        ));
        assert_eq!(h.state().tab, StoreTab::Flatpak);

        let px1 = h.render();
        assert_ne!(px0.data, px1.data);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
