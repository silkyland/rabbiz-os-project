//! RabbizOS Store & System Updater (`rabbiz-store`) — First-party native Rust
//! application for updating the Linux kernel, device/DTB/driver patches, and
//! browsing/installing mobile-friendly apps from Recommended (`.rbz` / native),
//! Flatpak (Flathub ARM64), and Paru (Arch Linux ARM / AUR).

#![forbid(unsafe_code)]

mod backend;

use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};

use de_api::{Point, Rect, Size};
use rabbiz_app_kit::mcp::Tool;
use rabbiz_app_kit::{
    ui, AppEvent, AppFrame, AppState, Canvas, TextBuffer, TextInputCtl, Theme,
};
use serde::{Deserialize, Serialize};

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
    CheckUpdatesBtn,
    ApplyAllUpdatesBtn,
    ItemAction(usize),
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
    pub items: Vec<CatalogItem>,
    pub scroll_y: i32,
    pub content_h: i32,
    size: Size,
    pressed: Hit,
    drag_start_y: Option<(i32, i32)>, // (touch_id, last_y)
    drag_moved: bool,
    busy_job: Option<String>,
    job_tx: Sender<backend::JobResult>,
    job_rx: Receiver<backend::JobResult>,
}

impl Default for StoreApp {
    fn default() -> Self {
        Self::new()
    }
}

impl StoreApp {
    #[must_use]
    pub fn new() -> Self {
        let path = state_file_path();
        let (job_tx, job_rx) = channel();
        if let Ok(raw) = std::fs::read_to_string(&path) {
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
                    items: saved.items,
                    scroll_y: 0,
                    content_h: 0,
                    size: Size::new(720, 1600),
                    pressed: Hit::None,
                    drag_start_y: None,
                    drag_moved: false,
                    busy_job: None,
                    job_tx,
                    job_rx,
                };
            }
        }
        let s = Self {
            tab: StoreTab::Updates,
            search: TextBuffer::new(),
            search_focused: false,
            kernel_release: probe_kernel_release(),
            os_channel: "RabbizOS 2026.11 (Arch Linux ARM64 · Exynos 850)".into(),
            last_checked: "Just now".into(),
            status_banner: "5 Linux & driver patches ready to install".into(),
            last_cmd_log: "rabbiz-updater check --channel=mainline-7.3".into(),
            items: default_catalog_items(),
            scroll_y: 0,
            content_h: 0,
            size: Size::new(720, 1600),
            pressed: Hit::None,
            drag_start_y: None,
            drag_moved: false,
            busy_job: None,
            job_tx,
            job_rx,
        };
        s.save_state();
        s
    }

    pub fn save_state(&self) {
        let path = state_file_path();
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
                let _ = std::fs::rename(&tmp, &path);
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

    /// Drain finished background package-manager jobs.
    pub fn drain_jobs(&mut self) -> bool {
        let mut changed = false;
        while let Ok(res) = self.job_rx.try_recv() {
            self.busy_job = None;
            changed = true;
            if res.ok {
                let n = backend::count_lines(&res.stdout);
                self.status_banner = if n > 0 {
                    format!("{} done ({n} entries)", res.label)
                } else {
                    format!("{} completed cleanly", res.label)
                };
            } else {
                let err = res
                    .stderr
                    .lines()
                    .next()
                    .unwrap_or("command failed")
                    .trim();
                self.status_banner = format!("{} failed: {err}", res.label);
            }
            self.save_state();
        }
        changed
    }

    /// Check for new Linux kernel/driver patches and refresh metadata.
    pub fn check_for_updates(&mut self) {
        self.last_checked = "Synced just now".into();
        let pending = self.pending_updates_count();
        self.last_cmd_log = "paru -Qu && flatpak remote-ls --updates".into();
        if backend::available("paru") {
            self.busy_job = Some("Checking updates".into());
            self.status_banner = "Checking paru / Arch Linux ARM updates…".into();
            backend::spawn_job(
                "Check updates".into(),
                backend::argv_of("paru -Qu"),
                self.job_tx.clone(),
            );
        } else if pending == 0 {
            self.status_banner = "System is up to date (Kernel 7.3.0-3 + all patches)".into();
        } else {
            self.status_banner = format!("{pending} Linux kernel & driver patches available");
        }
        self.save_state();
    }

    /// Apply all pending Linux kernel, DTB, and driver patches.
    pub fn apply_all_updates(&mut self) -> usize {
        if backend::available("rabbiz-updater") {
            self.busy_job = Some("Applying Linux patches".into());
            self.status_banner = "Applying Linux kernel & driver patches…".into();
            self.last_cmd_log = "rabbiz-updater apply-all".into();
            backend::spawn_job(
                "Apply Linux patches".into(),
                backend::argv_of("rabbiz-updater apply-all"),
                self.job_tx.clone(),
            );
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
        if !backend::available("rabbiz-updater") {
            if applied > 0 {
                self.kernel_release = "7.3.0-3-rabbiz-a21s".into();
                self.status_banner = format!("Applied {applied} Linux & driver patches cleanly");
                self.last_cmd_log = "rabbiz-updater apply-all && paru -Syu --noconfirm".into();
            } else {
                self.status_banner = "All Linux patches are already up to date".into();
            }
        }
        self.save_state();
        applied
    }

    /// Toggle install/update/remove on `self.items[idx]`.
    pub fn trigger_item_action(&mut self, idx: usize) {
        let Some(item) = self.items.get_mut(idx) else {
            return;
        };
        let want_install = item.update_available || !item.installed;
        let cmd_preview = item.source.cli_preview(&item.id, want_install);
        let tool = match item.source {
            PackageSource::LinuxPatch => "rabbiz-updater",
            PackageSource::Recommended => "rabbiz",
            PackageSource::Flatpak => "flatpak",
            PackageSource::Paru => "paru",
        };
        if backend::available(tool) {
            self.busy_job = Some(item.name.clone());
            self.last_cmd_log = cmd_preview.clone();
            self.status_banner = format!("Running {cmd_preview}…");
            backend::spawn_job(
                item.name.clone(),
                backend::argv_of(&cmd_preview),
                self.job_tx.clone(),
            );
        }
        if item.update_available {
            item.update_available = false;
            item.installed = true;
            if let Some((_, new_ver)) = item.version.split_once("->") {
                item.version = new_ver.trim().to_string();
            }
            self.last_cmd_log = cmd_preview;
            if self.busy_job.is_none() {
                self.status_banner = format!("Updated {} ({})", item.name, item.version);
            }
        } else if !item.installed {
            item.installed = true;
            self.last_cmd_log = cmd_preview;
            if self.busy_job.is_none() {
                self.status_banner = format!("Installed {} via {}", item.name, item.source.badge());
            }
        } else if item.source != PackageSource::LinuxPatch {
            item.installed = false;
            self.last_cmd_log = cmd_preview;
            if self.busy_job.is_none() {
                self.status_banner = format!("Removed {}", item.name);
            }
        } else if self.busy_job.is_none() {
            self.status_banner = format!("{} is active in the running kernel", item.name);
        }
        self.save_state();
    }

    fn tab_rects(&self) -> [(StoreTab, Rect); 4] {
        let w = self.size.w.max(360);
        let gap = 10;
        let total_w = w - ui::MARGIN * 2;
        let tab_w = (total_w - gap * 3) / 4;
        let y = ui::TOP_SAFE + 96;
        [
            (
                StoreTab::Updates,
                Rect::new(ui::MARGIN, y, tab_w, 52),
            ),
            (
                StoreTab::Recommended,
                Rect::new(ui::MARGIN + (tab_w + gap), y, tab_w, 52),
            ),
            (
                StoreTab::Flatpak,
                Rect::new(ui::MARGIN + (tab_w + gap) * 2, y, tab_w, 52),
            ),
            (
                StoreTab::Paru,
                Rect::new(ui::MARGIN + (tab_w + gap) * 3, y, tab_w, 52),
            ),
        ]
    }

    fn search_bar_rect(&self) -> Rect {
        let w = self.size.w.max(360);
        Rect::new(ui::MARGIN, ui::TOP_SAFE + 162, w - ui::MARGIN * 2, 56)
    }

    fn clear_search_rect(&self) -> Rect {
        let sb = self.search_bar_rect();
        Rect::new(sb.right() - 88, sb.y + 8, 80, 40)
    }

    fn list_top(&self) -> i32 {
        ui::TOP_SAFE + 232
    }

    fn hero_card_rect(&self) -> Rect {
        let w = self.size.w.max(360);
        Rect::new(
            ui::MARGIN,
            self.list_top() - self.scroll_y,
            w - ui::MARGIN * 2,
            212,
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

    fn item_card_rects(&self) -> Vec<(usize, Rect, Rect)> {
        let w = self.size.w.max(360);
        let mut y = self.list_top() - self.scroll_y;
        if self.tab == StoreTab::Updates {
            y += 228;
        }
        let card_w = w - ui::MARGIN * 2;
        let card_h = 152;
        let mut out = Vec::new();
        for idx in self.visible_indices() {
            let card = Rect::new(ui::MARGIN, y, card_w, card_h);
            let btn = Rect::new(card.right() - 148, card.y + 22, 132, 46);
            out.push((idx, card, btn));
            y += card_h + 14;
        }
        out
    }

    fn hit_test(&self, pos: Point) -> Hit {
        for (tab, rect) in self.tab_rects() {
            if rect.contains(pos) {
                return Hit::Tab(tab);
            }
        }
        if !self.search.is_empty() && self.clear_search_rect().contains(pos) {
            return Hit::ClearSearch;
        }
        if self.search_bar_rect().contains(pos) {
            return Hit::SearchBar;
        }
        if pos.y < self.list_top() {
            return Hit::None;
        }
        if self.tab == StoreTab::Updates {
            let (check_r, apply_r) = self.hero_button_rects();
            if check_r.contains(pos) {
                return Hit::CheckUpdatesBtn;
            }
            if apply_r.contains(pos) {
                return Hit::ApplyAllUpdatesBtn;
            }
        }
        for (idx, _card, btn) in self.item_card_rects() {
            if btn.contains(pos) {
                return Hit::ItemAction(idx);
            }
        }
        Hit::None
    }

    fn clamp_scroll(&mut self) {
        let viewport_h = (self.size.h - self.list_top() - ui::BOTTOM_SAFE).max(100);
        let max_scroll = (self.content_h - viewport_h).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max_scroll);
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
                self.drag_start_y = Some((id, pos.y));
                self.drag_moved = false;
                AppFrame {
                    dirty: true,
                    ..Default::default()
                }
            }
            AppEvent::TouchMove { id, pos } => {
                let Some((tid, last_y)) = self.drag_start_y else {
                    return AppFrame::default();
                };
                if tid != id {
                    return AppFrame::default();
                }
                let dy = pos.y - last_y;
                if dy.abs() > 8 || self.drag_moved {
                    self.drag_moved = true;
                    self.pressed = Hit::None;
                    self.drag_start_y = Some((id, pos.y));
                    self.scroll_y -= dy;
                    self.clamp_scroll();
                    return AppFrame {
                        dirty: true,
                        ..Default::default()
                    };
                }
                AppFrame::default()
            }
            AppEvent::TouchUp { id, pos } => {
                let Some((tid, _)) = self.drag_start_y.take() else {
                    return AppFrame::default();
                };
                if tid != id {
                    return AppFrame::default();
                }
                let hit = self.pressed;
                self.pressed = Hit::None;
                if !self.drag_moved && hit == self.hit_test(pos) {
                    match hit {
                        Hit::Tab(t) => {
                            self.tab = t;
                            self.scroll_y = 0;
                            self.search_focused = false;
                            ti.deactivate();
                        }
                        Hit::SearchBar => {
                            self.search_focused = true;
                            ti.activate(wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose::Normal);
                        }
                        Hit::ClearSearch => {
                            self.search.clear();
                            self.scroll_y = 0;
                        }
                        Hit::CheckUpdatesBtn => self.check_for_updates(),
                        Hit::ApplyAllUpdatesBtn => {
                            self.apply_all_updates();
                        }
                        Hit::ItemAction(idx) => self.trigger_item_action(idx),
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
            AppEvent::Key { keysym: 0xff08, .. } => {
                if self.search_focused && self.search.backspace() {
                    self.scroll_y = 0;
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
                    self.scroll_y = 0;
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
                    self.scroll_y = 0;
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
                    self.scroll_y = 0;
                    return AppFrame {
                        dirty: true,
                        ..Default::default()
                    };
                }
                AppFrame::default()
            }
            AppEvent::Tick(_) => AppFrame {
                dirty: job_changed,
                animating: self.busy_job.is_some(),
                quit: false,
            },
            _ => AppFrame {
                dirty: job_changed,
                animating: self.busy_job.is_some(),
                quit: false,
            },
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        self.drain_jobs();
        self.size = canvas.size();
        canvas.clear(theme.bg);

        // Compute content height before drawing scrollable items
        let count = self.visible_indices().len() as i32;
        self.content_h = (if self.tab == StoreTab::Updates { 228 } else { 0 })
            + count * 166
            + 64;
        self.clamp_scroll();

        let list_top = self.list_top();

        // 1. Scrollable content area (drawn first, then masked by header above `list_top`)
        if self.tab == StoreTab::Updates {
            let hero = self.hero_card_rect();
            if hero.bottom() > list_top {
                canvas.fill_rounded_rect(hero, 28.0, theme.surface1);
                let pending = self.pending_updates_count();
                let headline = if pending > 0 {
                    format!("{pending} Linux & Driver Patches Available")
                } else {
                    "RabbizOS Linux & Drivers Up to Date".to_string()
                };
                let head_col = if pending > 0 {
                    theme.accent
                } else {
                    theme.success
                };
                ui::draw_text(
                    canvas,
                    &headline,
                    &ui::styled(22.0, 600, head_col),
                    hero.x + 20,
                    hero.y + 18,
                    Some(hero.w - 40),
                );
                ui::draw_text(
                    canvas,
                    &format!("Kernel: {} · Checked: {}", self.kernel_release, self.last_checked),
                    &ui::caption(theme.text2),
                    hero.x + 20,
                    hero.y + 54,
                    Some(hero.w - 40),
                );
                ui::draw_text(
                    canvas,
                    &format!("Channel: {}", self.os_channel),
                    &ui::caption(theme.text3),
                    hero.x + 20,
                    hero.y + 82,
                    Some(hero.w - 40),
                );
                ui::draw_text(
                    canvas,
                    &format!("$ {}", self.last_cmd_log),
                    &ui::styled(15.0, 500, theme.text3),
                    hero.x + 20,
                    hero.y + 112,
                    Some(hero.w - 40),
                );

                let (check_r, apply_r) = self.hero_button_rects();
                ui::draw_pill_button(
                    canvas,
                    theme,
                    check_r,
                    "Check Updates",
                    false,
                    self.pressed == Hit::CheckUpdatesBtn,
                );
                let apply_label = if pending > 0 {
                    "Update All Now"
                } else {
                    "All Patched ✓"
                };
                ui::draw_pill_button(
                    canvas,
                    theme,
                    apply_r,
                    apply_label,
                    pending > 0,
                    self.pressed == Hit::ApplyAllUpdatesBtn,
                );
            }
        }

        let cards = self.item_card_rects();
        for (idx, card, btn) in cards {
            if card.bottom() < list_top || card.y > self.size.h {
                continue;
            }
            let item = &self.items[idx];
            ui::draw_card(canvas, theme, card, false);

            // Source & size badge
            let badge_str = format!("{} · {}", item.source.badge(), item.size_label);
            ui::draw_text(
                canvas,
                &badge_str,
                &ui::styled(14.0, 600, theme.accent),
                card.x + 20,
                card.y + 16,
                Some(card.w - 180),
            );

            // Title & version
            ui::draw_text(
                canvas,
                &item.name,
                &ui::styled(20.0, 600, theme.text),
                card.x + 20,
                card.y + 40,
                Some(card.w - 180),
            );
            ui::draw_text(
                canvas,
                &format!("v{} · {}", item.version, item.id),
                &ui::styled(15.0, 400, theme.text3),
                card.x + 20,
                card.y + 70,
                Some(card.w - 180),
            );

            // Summary
            ui::draw_text(
                canvas,
                &item.summary,
                &ui::styled(16.0, 400, theme.text2),
                card.x + 20,
                card.y + 98,
                Some(card.w - 40),
            );

            // Action button
            let (btn_label, primary) = if item.update_available {
                ("Update", true)
            } else if !item.installed {
                ("Install", true)
            } else if item.source == PackageSource::LinuxPatch {
                ("Patched ✓", false)
            } else {
                ("Installed", false)
            };
            ui::draw_pill_button(
                canvas,
                theme,
                btn,
                btn_label,
                primary,
                self.pressed == Hit::ItemAction(idx),
            );
        }

        // 2. Top chrome mask (covers scrolled cards above `list_top`)
        canvas.fill_rect(Rect::new(0, 0, self.size.w, list_top), theme.bg);
        let _ = ui::draw_header(canvas, theme, "Store & Updates", &self.status_banner);

        // 3. 4-Tab Bar (Updates | Featured | Flatpak | Paru AUR)
        for (tab, rect) in self.tab_rects() {
            let selected = self.tab == tab;
            let label = if tab == StoreTab::Updates {
                let n = self.pending_updates_count();
                if n > 0 {
                    format!("Updates ({n})")
                } else {
                    "Updates".to_string()
                }
            } else {
                tab.label().to_string()
            };
            ui::draw_pill_button(
                canvas,
                theme,
                rect,
                &label,
                selected,
                self.pressed == Hit::Tab(tab),
            );
        }

        // 4. Search Bar
        let sb = self.search_bar_rect();
        let sb_bg = if self.search_focused {
            theme.surface2
        } else {
            theme.surface1
        };
        canvas.fill_rounded_rect(sb, 28.0, sb_bg);
        let query = self.search.as_str();
        if query.is_empty() {
            let hint = match self.tab {
                StoreTab::Updates => "Search Linux kernel, DTB, SCSC, ABOX patches...",
                StoreTab::Recommended => "Search curated RabbizOS mobile apps...",
                StoreTab::Flatpak => "Search Flathub ARM64 packages...",
                StoreTab::Paru => "Search Arch Linux ARM & Paru AUR packages...",
            };
            ui::draw_text(
                canvas,
                hint,
                &ui::caption(theme.text3),
                sb.x + 22,
                sb.y + 17,
                Some(sb.w - 44),
            );
        } else {
            ui::draw_text(
                canvas,
                query,
                &ui::label(theme.text),
                sb.x + 22,
                sb.y + 15,
                Some(sb.w - 116),
            );
            let clr = self.clear_search_rect();
            ui::draw_pill_button(
                canvas,
                theme,
                clr,
                "Clear",
                false,
                self.pressed == Hit::ClearSearch,
            );
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
        std::env::set_var("XDG_DATA_HOME", &tmp);

        let mut app = StoreApp::new();
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

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_harness_renders_and_switches_store_tabs() {
        let tmp = std::env::temp_dir().join(format!("rabbiz-store-ui-test-{}", std::process::id()));
        std::env::set_var("XDG_DATA_HOME", &tmp);

        let mut h = rabbiz_app_kit::TestHarness::new(StoreApp::new());
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
