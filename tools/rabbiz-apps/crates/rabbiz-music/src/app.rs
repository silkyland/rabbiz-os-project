//! Music GUI application state, layout geometry, hit-testing, and input handling.

use std::path::PathBuf;
use std::time::Instant;

use de_design::motion::{bezier_eval, BEZIER_STANDARD, RUBBER_FACTOR};
use rabbiz_app_kit::{ui, AppEvent, AppFrame, AppState, Canvas, Point, Rect, TextInputCtl, Theme};

use crate::library::{self, next_index, prev_index, Scope, Track};
use crate::playback::Player;
use crate::player::mpv_available;

// Row metrics (pitch includes the gap).
pub(crate) const ROW_H: i32 = 76;
pub(crate) const ROW_PITCH: i32 = 84;
/// Now-playing card height (art row + seek + times + transport + padding).
pub(crate) const NP_H: i32 = 272;
/// Seek thumb radius / track height.
pub(crate) const SEEK_THUMB: i32 = 9;
pub(crate) const SEEK_TRACK_H: i32 = 8;
/// Scroll settle duration — inside the ≤600 ms motion budget, on the
/// `DURATIONS_MS` 160/240 ladder.
const SCROLL_SETTLE_S: f32 = 0.18;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Rescan,
    Chip(Scope),
    Row(usize), // index into `tracks`
    Seek,
    Prev,
    Toggle,
    Next,
    Stop,
    EmptyAction,
}

pub(crate) struct MusicApp {
    pub(crate) tracks: Vec<Track>,
    pub(crate) scope: Scope,
    pub(crate) player: Player,
    /// Last rendered size — hit-testing uses it between frames.
    vp: (i32, i32),
    /// List scroll offset px (content pixels).
    pub(crate) scroll: i32,
    max_scroll: i32,
    /// List drag: (start_y, scroll_at_start, moved).
    drag: Option<(i32, i32, bool)>,
    /// Scroll settle animation: (from, to, elapsed s).
    scroll_anim: Option<(f32, f32, f32)>,
    /// Seek drag: (touch id, raw fraction — may leave 0..=1 for rubber band).
    pub(crate) seek_drag: Option<(i32, f32)>,
    pub(crate) pressed: Option<Hit>,
    last_instant: Option<Instant>,
}

/// Everything `render` needs that `hit_at` must agree on.
pub(crate) struct Layout {
    pub(crate) header_bottom: i32,
    pub(crate) rescan: Rect,
    pub(crate) chips: [Rect; 3],
    pub(crate) list: Rect,
    pub(crate) np: Option<Rect>,
    pub(crate) empty_btn: Option<Rect>,
}

/// Rubber-band response past a `[0, max]` limit (de-design §7.17a):
/// displacement = RUBBER_FACTOR × overshoot.
pub(crate) fn rubber_scroll(raw: i32, max: i32) -> i32 {
    if raw < 0 {
        (raw as f32 * RUBBER_FACTOR) as i32
    } else if raw > max {
        max + ((raw - max) as f32 * RUBBER_FACTOR) as i32
    } else {
        raw
    }
}

/// Seek-bar rubber: fraction may leave 0..=1 while dragging.
pub(crate) fn rubber_frac(raw: f32) -> f32 {
    if raw < 0.0 {
        raw * RUBBER_FACTOR
    } else if raw > 1.0 {
        1.0 + (raw - 1.0) * RUBBER_FACTOR
    } else {
        raw
    }
}

impl MusicApp {
    pub(crate) fn new() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/home/rabbiz"));
        let scan = library::scan(&home);
        Self {
            tracks: scan.tracks,
            scope: Scope::All,
            player: Player::new(mpv_available()),
            vp: (720, 1600),
            scroll: 0,
            max_scroll: 0,
            drag: None,
            scroll_anim: None,
            seek_drag: None,
            pressed: None,
            last_instant: None,
        }
    }

    pub(crate) fn visible(&self) -> Vec<usize> {
        library::visible_indices(&self.tracks, self.scope)
    }

    // ---------------- wall-clock pump (runs on events AND render) ---------

    fn pump(&mut self) -> bool {
        let now = Instant::now();
        let dt = self
            .last_instant
            .map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f32());
        self.last_instant = Some(now);
        let mut changed = false;

        if let Some((from, to, t)) = self.scroll_anim {
            let nt = t + dt / SCROLL_SETTLE_S;
            if nt >= 1.0 {
                self.scroll = to.round() as i32;
                self.scroll_anim = None;
            } else {
                let e = bezier_eval(BEZIER_STANDARD, nt);
                self.scroll = (from + (to - from) * e).round() as i32;
                self.scroll_anim = Some((from, to, nt));
            }
            changed = true;
        }
        changed |= self.player.pump(&self.tracks);
        changed
    }

    // ---------------- layout ----------------------------------------------

    pub(crate) fn layout(&self, w: i32, h: i32) -> Layout {
        // draw_header returns TOP_SAFE+16 + 44 + 24 when a subtitle is set.
        let header_bottom = ui::TOP_SAFE + 16 + 80;

        let rescan = Rect::new(w - ui::MARGIN - 48, ui::TOP_SAFE + 16, 48, 48);
        let mut y = header_bottom + 8;
        let mut banner_h = 0;
        if !self.player.mpv_ok {
            banner_h += 88 + 8;
        }
        if self.player.error.is_some() {
            banner_h += 88 + 8;
        }
        y += banner_h;

        let mut chips = [Rect::ZERO; 3];
        let mut empty_btn = None;
        if !self.tracks.is_empty() {
            let gap = 8;
            let cw = (w - ui::MARGIN * 2 - gap * 2) / 3;
            for (i, chip) in chips.iter_mut().enumerate() {
                *chip = Rect::new(ui::MARGIN + i as i32 * (cw + gap), y, cw, 48);
            }
            y += 48 + 8 + 26; // chips + gap + section label line
        } else {
            y += 8;
        }

        let np = if self.player.has_track() {
            Some(Rect::new(
                ui::MARGIN,
                h - ui::BOTTOM_SAFE - 8 - NP_H,
                w - ui::MARGIN * 2,
                NP_H,
            ))
        } else {
            None
        };
        let list_bottom = np.map_or(h - ui::BOTTOM_SAFE - 8, |r| r.y - 8);
        let list = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, (list_bottom - y).max(80));

        if self.tracks.is_empty() || self.visible().is_empty() {
            empty_btn = Some(Rect::new(w / 2 - 110, list.y + list.h / 2 + 90, 220, 48));
        }

        Layout {
            header_bottom,
            rescan,
            chips,
            list,
            np,
            empty_btn,
        }
    }

    fn subtitle(&self) -> String {
        if self.tracks.is_empty() {
            return "No audio in ~/Music or ~/Downloads".into();
        }
        let n = self.tracks.len();
        format!("{n} track{} · newest first", if n == 1 { "" } else { "s" })
    }

    fn update_scroll_limits(&mut self, w: i32, h: i32) {
        let l = self.layout(w, h);
        let content = self.visible().len() as i32 * ROW_PITCH;
        self.max_scroll = (content - l.list.h).max(0);
        // Rubber-band drags and the settle animation may legitimately sit
        // outside [0, max_scroll]; only snap when idle.
        if self.drag.is_none() && self.scroll_anim.is_none() {
            self.scroll = self.scroll.clamp(0, self.max_scroll);
        }
    }

    // ---------------- hit testing ------------------------------------------

    pub(crate) fn seek_touch_rect(np: Rect) -> Rect {
        Rect::new(np.x + 16, np.y + 16 + 72 + 10, np.w - 32, 44)
    }

    pub(crate) fn seek_track_rect(np: Rect) -> Rect {
        let t = Self::seek_touch_rect(np);
        Rect::new(t.x, t.y + (t.h - SEEK_TRACK_H) / 2, t.w, SEEK_TRACK_H)
    }

    pub(crate) fn transport_rects(np: Rect) -> [Rect; 3] {
        // Row of prev(56) / play(72) / next(56) centered in the card's
        // bottom 84 px band.
        let y = np.y + NP_H - 16 - 84;
        let total = 56 + 28 + 72 + 28 + 56;
        let x0 = np.x + (np.w - total) / 2;
        [
            Rect::new(x0, y + 14, 56, 56),
            Rect::new(x0 + 84, y, 72, 72),
            Rect::new(x0 + 184, y + 14, 56, 56),
        ]
    }

    pub(crate) fn stop_rect(np: Rect) -> Rect {
        Rect::new(np.x + np.w - 16 - 44, np.y + 16, 44, 44)
    }

    /// Fraction (0..=1, unclamped for rubber) of a point inside the seek
    /// track rect.
    fn seek_fraction(np: Rect, pos: Point) -> f32 {
        let t = Self::seek_track_rect(np);
        if t.w <= 0 {
            return 0.0;
        }
        (pos.x - t.x) as f32 / t.w as f32
    }

    fn hit_at(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let l = self.layout(w, h);
        // Now-playing zone first (it sits below the list; no overlap anyway).
        if let Some(np) = l.np {
            if Self::stop_rect(np).contains(pos) {
                return Some(Hit::Stop);
            }
            if Self::seek_touch_rect(np).contains(pos) {
                return Some(Hit::Seek);
            }
            for (i, r) in Self::transport_rects(np).iter().enumerate() {
                if r.contains(pos) {
                    return Some([Hit::Prev, Hit::Toggle, Hit::Next][i]);
                }
            }
        }
        // Track rows inside the list viewport.
        if l.list.contains(pos) {
            let vis = self.visible();
            let row_i = (pos.y - l.list.y + self.scroll) / ROW_PITCH;
            if row_i >= 0 && (row_i as usize) < vis.len() {
                let track_idx = vis[row_i as usize];
                let row_top = l.list.y + row_i * ROW_PITCH - self.scroll;
                if pos.y >= row_top && pos.y < row_top + ROW_H {
                    return Some(Hit::Row(track_idx));
                }
            }
            return None; // inside list but not on a row → scroll drag only
        }
        for (i, r) in l.chips.iter().enumerate() {
            if r.contains(pos) {
                return Some(Hit::Chip(Scope::ALL[i]));
            }
        }
        if l.rescan.contains(pos) {
            return Some(Hit::Rescan);
        }
        if l.empty_btn.is_some_and(|r| r.contains(pos)) {
            return Some(Hit::EmptyAction);
        }
        None
    }

    // ---------------- actions ----------------------------------------------

    /// Central play/pause entry: with nothing loaded, play the first track
    /// visible under the current scope; otherwise toggle.
    fn toggle_play(&mut self) {
        if self.player.has_track() {
            self.player.toggle(&self.tracks);
        } else if let Some(&i) = self.visible().first() {
            self.player.play_track(i, &self.tracks);
        }
    }

    fn rescan(&mut self) {
        // Keep `current` pointed at the same file across re-sorts: match by
        // path, not index. A vanished file stops playback honestly.
        let old_path = self
            .player
            .current
            .and_then(|i| self.tracks.get(i))
            .map(|t| t.path.clone());
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/home/rabbiz"));
        let scan = library::scan(&home);
        self.tracks = scan.tracks;
        if let Some(p) = old_path {
            match self.tracks.iter().position(|t| t.path == p) {
                Some(i) => self.player.current = Some(i),
                None => self.player.stop(),
            }
        }
        self.scroll = 0;
        self.scroll_anim = None;
    }

    fn dispatch(&mut self, hit: Hit) {
        match hit {
            Hit::Rescan => self.rescan(),
            Hit::Chip(s) => {
                self.scope = s;
                self.scroll = 0;
                self.scroll_anim = None;
            }
            Hit::Row(i) => {
                if self.player.current == Some(i) {
                    self.player.toggle(&self.tracks);
                } else {
                    self.player.play_track(i, &self.tracks);
                }
            }
            Hit::Seek => {} // handled by seek_drag on release
            Hit::Prev => {
                if let Some(i) = prev_index(self.player.current, self.tracks.len()) {
                    self.player.play_track(i, &self.tracks);
                }
            }
            Hit::Next => {
                if let Some(i) = next_index(self.player.current, self.tracks.len()) {
                    self.player.play_track(i, &self.tracks);
                }
            }
            Hit::Toggle => self.toggle_play(),
            Hit::Stop => self.player.stop(),
            Hit::EmptyAction => {
                if self.tracks.is_empty() {
                    self.rescan();
                } else {
                    self.scope = Scope::All;
                    self.scroll = 0;
                }
            }
        }
    }
}

impl AppState for MusicApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-music"
    }
    fn title(&self) -> &'static str {
        "Music"
    }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        let changed = self.pump();
        let (w, h) = self.vp;
        let mut dirty = changed;

        match event {
            AppEvent::TouchDown { id, pos } => {
                self.update_scroll_limits(w, h);
                let hit = self.hit_at(w, h, pos);
                self.pressed = hit;
                self.seek_drag = None;
                self.drag = None;
                // Seek drag captures the touch anywhere in its band.
                if hit == Some(Hit::Seek) {
                    if let Some(np) = self.layout(w, h).np {
                        self.seek_drag = Some((id, Self::seek_fraction(np, pos)));
                    }
                } else if self.layout(w, h).list.contains(pos) {
                    self.drag = Some((pos.y, self.scroll, false));
                }
                dirty = true;
            }
            AppEvent::TouchMove { id, pos } => {
                if let Some((did, _)) = self.seek_drag {
                    if did == id {
                        if let Some(np) = self.layout(w, h).np {
                            self.seek_drag = Some((id, Self::seek_fraction(np, pos)));
                            dirty = true;
                        }
                    }
                } else if let Some((sy, ss, _)) = self.drag {
                    if (sy - pos.y).abs() > 10 {
                        self.drag = Some((sy, ss, true));
                        self.pressed = None;
                    }
                    let raw = ss + sy - pos.y;
                    self.scroll_anim = None;
                    self.scroll = rubber_scroll(raw, self.max_scroll);
                    dirty = true;
                }
            }
            AppEvent::TouchUp { id, pos } => {
                // Seek release: commit the (clamped) fraction.
                if let Some((did, raw)) = self.seek_drag.take() {
                    if did == id {
                        self.player.seek_to_fraction(raw.clamp(0.0, 1.0));
                        self.pressed = None;
                        dirty = true;
                    }
                } else if let Some((_, _, moved)) = self.drag.take() {
                    self.pressed = None;
                    if !moved {
                        if let Some(hit) = self.hit_at(w, h, pos) {
                            self.dispatch(hit);
                        }
                    }
                    // Settle back inside bounds with a short ease.
                    let clamped = self.scroll.clamp(0, self.max_scroll);
                    if (self.scroll - clamped).abs() > 1 {
                        self.scroll_anim = Some((self.scroll as f32, clamped as f32, 0.0));
                    } else {
                        self.scroll = clamped;
                    }
                    dirty = true;
                } else {
                    let hit = self.pressed.take();
                    self.pressed = None;
                    if let Some(hit) = hit {
                        if self.hit_at(w, h, pos) == Some(hit) {
                            self.dispatch(hit);
                        }
                    }
                    dirty = true;
                }
            }
            AppEvent::Key { utf8: Some(s), .. } if s == " " => {
                self.toggle_play();
                dirty = true;
            }
            AppEvent::Tick(_) => dirty = true,
            _ => {}
        }

        AppFrame {
            dirty,
            animating: self.player.has_track() || self.scroll_anim.is_some() || self.drag.is_some(),
            quit: false,
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let changed = self.pump();
        let _ = changed;
        let size = canvas.size();
        let (w, h) = (size.w, size.h);
        self.vp = (w, h);
        self.update_scroll_limits(w, h);
        let l = self.layout(w, h);

        ui::draw_header(canvas, theme, "Music", &self.subtitle());
        self.draw_rescan(canvas, theme, l.rescan);
        self.draw_banners(canvas, theme, l.header_bottom);
        self.draw_chips(canvas, theme, &l);
        self.draw_section_label(canvas, theme, &l);
        self.draw_list(canvas, theme, &l);
        self.draw_empty(canvas, theme, &l);
        if let Some(np) = l.np {
            self.draw_now_playing(canvas, theme, np);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rubber_frac_damps_past_edges() {
        assert_eq!(rubber_frac(0.5), 0.5);
        assert_eq!(rubber_frac(0.0), 0.0);
        assert_eq!(rubber_frac(1.0), 1.0);
        // -0.5 past the start → -0.275 displayed.
        assert!((rubber_frac(-0.5) - (-0.5 * RUBBER_FACTOR)).abs() < 1e-6);
        // 1.5 → 1 + 0.5*0.55.
        assert!((rubber_frac(1.5) - (1.0 + 0.5 * RUBBER_FACTOR)).abs() < 1e-6);
    }

    #[test]
    fn rubber_scroll_respects_bounds() {
        assert_eq!(rubber_scroll(50, 100), 50);
        assert_eq!(rubber_scroll(-100, 100), (-100.0 * RUBBER_FACTOR) as i32);
        assert_eq!(
            rubber_scroll(200, 100),
            100 + ((100.0) * RUBBER_FACTOR) as i32
        );
    }

    #[test]
    fn seek_fraction_maps_track_width() {
        let np = Rect::new(24, 1200, 672, NP_H);
        let t = MusicApp::seek_track_rect(np);
        let mid = Point::new(t.x + t.w / 2, t.y + t.h / 2);
        assert!((MusicApp::seek_fraction(np, mid) - 0.5).abs() < 0.01);
        let left = Point::new(t.x - 40, mid.y);
        assert!(MusicApp::seek_fraction(np, left) < 0.0);
        let right = Point::new(t.x + t.w + 40, mid.y);
        assert!(MusicApp::seek_fraction(np, right) > 1.0);
    }

    #[test]
    fn transport_rects_are_distinct_and_inside_card() {
        let np = Rect::new(24, 1200, 672, NP_H);
        let [a, b, c] = MusicApp::transport_rects(np);
        assert!(a.w == 56 && b.w == 72 && c.w == 56);
        assert!(a.x < b.x && b.x < c.x);
        assert!(b.y >= np.y && c.y + c.h <= np.y + np.h);
    }

    #[test]
    fn layout_never_hardcodes_panel_size() {
        let app = MusicApp::new();
        for (w, h) in [(720, 1600), (1080, 2400), (540, 960)] {
            let l = app.layout(w, h);
            assert!(l.list.x >= ui::MARGIN);
            assert!(l.list.x + l.list.w <= w - ui::MARGIN + 1);
            assert!(l.list.y < l.list.y + l.list.h);
            if let Some(np) = l.np {
                assert!(np.y + np.h <= h - ui::BOTTOM_SAFE);
            }
        }
    }
}
