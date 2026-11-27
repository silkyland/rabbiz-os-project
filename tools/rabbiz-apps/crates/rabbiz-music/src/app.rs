//! Music GUI application state, layout geometry, hit-testing, and input handling.
//!
//! Material You media conventions: a `TopAppBar` library screen (title +
//! rescan action, scope chips, flat track rows with inset dividers) and a
//! ~104 px mini-player pinned above the gesture bar that expands on tap into
//! a fullscreen player (big art, slider seek, 5-way transport with real
//! shuffle/repeat). The list scrolls through `ScrollView` (fling + rubber
//! band); the expand is a ~300 ms fade+translate driven by `AppEvent::Tick`.

use std::path::PathBuf;
use std::time::Instant;

use de_design::motion::{bezier_eval, BEZIER_STANDARD, RUBBER_FACTOR};
use rabbiz_app_kit::widgets::{self, TopAppBar};
use rabbiz_app_kit::{
    ui, AppEvent, AppFrame, AppState, Canvas, Point, Rect, ScrollView, Size, TextInputCtl, Theme,
};

use crate::library::{self, next_index, prev_index, shuffle_next_index, Scope, Track};
use crate::playback::Player;
use crate::player::mpv_available;

/// Flat list row height (title + caption + 56 px art tile).
pub(crate) const ROW_H: i32 = 84;
/// Mini-player band height, including the 3 px progress hairline.
pub(crate) const MINI_H: i32 = 104;
/// Status banner (mpv missing / playback error) height and vertical gap.
pub(crate) const BANNER_H: i32 = 64;
const BANNER_GAP: i32 = 8;
/// Mini → fullscreen expand/collapse duration — inside the ≤600 ms motion
/// budget, on the emphasized 300 ms rung.
const EXPAND_S: f32 = 0.30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Rescan,
    Chip(Scope),
    Row(usize), // index into `tracks`
    Seek,
    Prev,
    Toggle,
    Next,
    Shuffle,
    RepeatBtn,
    /// Mini-player body — expands to the fullscreen player.
    ExpandPlayer,
    /// Fullscreen player top chevron — collapses back to the library.
    CollapsePlayer,
    MiniToggle,
    MiniNext,
    EmptyAction,
}

pub(crate) struct MusicApp {
    pub(crate) tracks: Vec<Track>,
    pub(crate) scope: Scope,
    pub(crate) player: Player,
    /// Last rendered size — hit-testing uses it between frames.
    vp: (i32, i32),
    /// Library list scroll physics (fling + rubber-band overscroll).
    pub(crate) list_sv: ScrollView,
    /// Touch id that went down inside the list viewport.
    list_touch: Option<i32>,
    /// Monotonic clock for ScrollView velocity timestamps.
    t0: Instant,
    /// Seek drag: (touch id, raw fraction — may leave 0..=1 for rubber band).
    pub(crate) seek_drag: Option<(i32, f32)>,
    /// Fullscreen player is the input owner (mini is its collapsed form).
    player_open: bool,
    /// 0.0 = mini docked, 1.0 = fullscreen (animated by `expand_anim`).
    pub(crate) expand_v: f32,
    /// Expand tween: (from, to, progress 0..=1).
    expand_anim: Option<(f32, f32, f32)>,
    /// Scope-chip rects measured at render time — hit-testing matches pixels.
    pub(crate) chip_rects: Vec<Rect>,
    pub(crate) pressed: Option<Hit>,
}

/// Everything `render` needs for the library screen (hit-testing agrees).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Layout {
    pub(crate) bar: Rect,
    pub(crate) rescan: Rect,
    /// Top y of the first status banner (banners stack with `BANNER_GAP`).
    pub(crate) banner_top: i32,
    /// Top y of the scope chip row (0 when the library is empty).
    pub(crate) chips_y: i32,
    /// Top y of the "Library / N tracks" section label line.
    pub(crate) label_y: i32,
    pub(crate) list: Rect,
    pub(crate) mini: Option<Rect>,
    pub(crate) empty_btn: Option<Rect>,
}

/// Fullscreen-player geometry, shared by draw + hit-test.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlayerGeom {
    pub(crate) bar: Rect,
    pub(crate) collapse: Rect,
    pub(crate) art: Rect,
    pub(crate) title: Rect,
    pub(crate) subtitle: Rect,
    /// Slider lane (also the seek touch target, ~48 px tall).
    pub(crate) seek_lane: Rect,
    pub(crate) times: Rect,
    /// shuffle / prev / hero / next / repeat.
    pub(crate) transport: [Rect; 5],
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

fn now_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_nanos() & 0xFFFF_FFFF_FFFF_FFFF) as u64)
        .unwrap_or(0x9E37_79B9)
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
            list_sv: ScrollView::new(),
            list_touch: None,
            t0: Instant::now(),
            seek_drag: None,
            player_open: false,
            expand_v: 0.0,
            expand_anim: None,
            chip_rects: Vec::new(),
            pressed: None,
        }
    }

    pub(crate) fn visible(&self) -> Vec<usize> {
        library::visible_indices(&self.tracks, self.scope)
    }

    fn now_ms(&self) -> u64 {
        self.t0.elapsed().as_millis() as u64
    }

    // ---------------- wall-clock pump (mpv IPC; anims tick via Tick dt) ---

    fn pump(&mut self) -> bool {
        self.player.pump(&self.tracks)
    }

    /// Advance the expand tween by `dt` seconds. Returns true while running.
    fn tick_expand(&mut self, dt: f32) -> bool {
        let Some((from, to, t)) = self.expand_anim else {
            return false;
        };
        let nt = t + dt / EXPAND_S;
        if nt >= 1.0 {
            self.expand_v = to;
            self.expand_anim = None;
        } else {
            let e = bezier_eval(BEZIER_STANDARD, nt);
            self.expand_v = from + (to - from) * e;
            self.expand_anim = Some((from, to, nt));
        }
        true
    }

    // ---------------- layout ----------------------------------------------

    pub(crate) fn layout(&self, w: i32, h: i32) -> Layout {
        let size = Size::new(w, h);
        let bar = TopAppBar::layout(size, ui::TOP_SAFE);
        let rescan = TopAppBar::action_rect(bar);

        // Banners: each reserves BANNER_H + BANNER_GAP below the bar —
        // `draw_banners` paints at exactly this top, same step. (The old
        // code reserved 96/banner but painted at header_bottom, an 8 px
        // mismatch that nudged the chips row off-grid.)
        let banner_top = bar.bottom() + BANNER_GAP;
        let mut y = banner_top;
        if !self.player.mpv_ok {
            y += BANNER_H + BANNER_GAP;
        }
        if self.player.error.is_some() {
            y += BANNER_H + BANNER_GAP;
        }

        let mut chips_y = 0;
        let mut label_y = 0;
        if !self.tracks.is_empty() {
            chips_y = y;
            label_y = y + widgets::CHIP_H + 10;
            y = label_y + 24 + 8;
        } else {
            y += 8;
        }

        let mini = if self.player.has_track() {
            Some(Rect::new(0, h - ui::BOTTOM_SAFE - MINI_H, w, MINI_H))
        } else {
            None
        };
        let list_bottom = mini.map_or(h - ui::BOTTOM_SAFE, |r| r.y - 8);
        // Flat rows span the full width; content indents by ui::MARGIN.
        let list = Rect::new(0, y, w, (list_bottom - y).max(80));

        let mut empty_btn = None;
        if self.tracks.is_empty() || self.visible().is_empty() {
            empty_btn = Some(Rect::new(w / 2 - 110, list.y + list.h / 2 + 100, 220, 48));
        }

        Layout {
            bar,
            rescan,
            banner_top,
            chips_y,
            label_y,
            list,
            mini,
            empty_btn,
        }
    }

    /// Fullscreen-player geometry for a `w`×`h` surface.
    pub(crate) fn player_geom(&self, w: i32, h: i32) -> PlayerGeom {
        let size = Size::new(w, h);
        let bar = TopAppBar::layout(size, ui::TOP_SAFE);
        let collapse = TopAppBar::back_rect(bar);

        // Reserve the track details and transport before sizing the art.
        let controls_h = 28 + 30 + 44 + 30 + 36 + 48 + 2 + 24 + 56 + 96;
        let art_room = (h - ui::BOTTOM_SAFE - bar.bottom() - controls_h).max(0);
        let side = (w * 4 / 5).min(h * 36 / 100).min(576).min(art_room);
        let art = Rect::new((w - side) / 2, bar.bottom() + 28, side, side);
        let title = Rect::new(ui::MARGIN, art.bottom() + 30, w - ui::MARGIN * 2, 44);
        let subtitle = Rect::new(ui::MARGIN, title.bottom(), w - ui::MARGIN * 2, 30);
        let seek_lane = Rect::new(
            ui::MARGIN + 8,
            subtitle.bottom() + 36,
            w - (ui::MARGIN + 8) * 2,
            48,
        );
        let times = Rect::new(seek_lane.x, seek_lane.bottom() + 2, seek_lane.w, 24);

        // Transport: shuffle(56) prev(64) hero(96) next(64) repeat(56),
        // evenly spaced and centered on the hero.
        let sizes = if w >= 360 {
            [56, 64, 96, 64, 56]
        } else {
            [48, 48, 72, 48, 48]
        };
        let gap = ((w - sizes.iter().sum::<i32>() - ui::MARGIN * 2) / 4).clamp(0, 32);
        let total: i32 = sizes.iter().sum::<i32>() + gap * 4;
        let ty = times.bottom() + 56;
        let mut transport = [Rect::ZERO; 5];
        let mut tx = (w - total) / 2;
        for (i, s) in sizes.iter().enumerate() {
            transport[i] = Rect::new(tx, ty + (sizes[2] - s) / 2, *s, *s);
            tx += s + gap;
        }

        PlayerGeom {
            bar,
            collapse,
            art,
            title,
            subtitle,
            seek_lane,
            times,
            transport,
        }
    }

    // ---------------- mini-player geometry ---------------------------------

    pub(crate) fn mini_tile_rect(np: Rect) -> Rect {
        Rect::new(np.x + 16, np.y + (np.h - 56) / 2 + 2, 56, 56)
    }

    pub(crate) fn mini_play_rect(np: Rect) -> Rect {
        Rect::new(np.right() - 16 - 48, np.y + (np.h - 48) / 2 + 2, 48, 48)
    }

    pub(crate) fn mini_next_rect(np: Rect) -> Rect {
        Rect::new(np.right() - 16 - 96, np.y + (np.h - 48) / 2 + 2, 48, 48)
    }

    pub(crate) fn mini_text_rect(np: Rect) -> Rect {
        let tile = Self::mini_tile_rect(np);
        let next = Self::mini_next_rect(np);
        Rect::new(
            tile.right() + 16,
            np.y,
            (next.x - 12 - tile.right() - 16).max(0),
            np.h,
        )
    }

    /// Fraction (0..=1, unclamped for rubber) of a point inside the seek lane.
    pub(crate) fn seek_fraction(lane: Rect, pos: Point) -> f32 {
        if lane.w <= 0 {
            return 0.0;
        }
        (pos.x - lane.x) as f32 / lane.w as f32
    }

    // ---------------- hit testing ------------------------------------------

    fn hit_at(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        if self.player_open {
            let g = self.player_geom(w, h);
            if g.collapse.contains(pos) {
                return Some(Hit::CollapsePlayer);
            }
            if g.seek_lane.contains(pos) {
                return Some(Hit::Seek);
            }
            for (i, r) in g.transport.iter().enumerate() {
                if r.contains(pos) {
                    return Some(
                        [
                            Hit::Shuffle,
                            Hit::Prev,
                            Hit::Toggle,
                            Hit::Next,
                            Hit::RepeatBtn,
                        ][i],
                    );
                }
            }
            return None;
        }

        let l = self.layout(w, h);
        // Mini-player first: its buttons, then the whole body expands.
        if let Some(np) = l.mini {
            if Self::mini_play_rect(np).contains(pos) {
                return Some(Hit::MiniToggle);
            }
            if Self::mini_next_rect(np).contains(pos) {
                return Some(Hit::MiniNext);
            }
            if np.contains(pos) {
                return Some(Hit::ExpandPlayer);
            }
        }
        // Track rows inside the list viewport (rows are contiguous, ROW_H
        // pitch, divider painted inside the row bottom).
        if l.list.contains(pos) {
            let vis = self.visible();
            let row_i = (pos.y - l.list.y + self.list_sv.offset_i32()) / ROW_H;
            if row_i >= 0 && (row_i as usize) < vis.len() {
                return Some(Hit::Row(vis[row_i as usize]));
            }
            return None; // inside list but past the last row → scroll drag only
        }
        for (i, r) in self.chip_rects.iter().enumerate() {
            if r.contains(pos) {
                return Some(Hit::Chip(Scope::ALL[i.min(Scope::ALL.len() - 1)]));
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

    /// Next-track honoring shuffle (random pick ≠ current) vs sequential.
    fn next_track(&mut self) {
        let n = self.tracks.len();
        let i = if self.player.shuffle {
            shuffle_next_index(self.player.current, n, now_seed())
        } else {
            next_index(self.player.current, n)
        };
        if let Some(i) = i {
            self.player.play_track(i, &self.tracks);
        }
    }

    fn open_player(&mut self) {
        if !self.player.has_track() {
            return;
        }
        self.player_open = true;
        self.expand_anim = Some((self.expand_v, 1.0, 0.0));
    }

    fn close_player(&mut self) {
        self.player_open = false;
        self.expand_anim = Some((self.expand_v, 0.0, 0.0));
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
        self.list_sv.scroll_to(0.0);
    }

    fn dispatch(&mut self, hit: Hit) {
        match hit {
            Hit::Rescan => self.rescan(),
            Hit::Chip(s) => {
                self.scope = s;
                self.list_sv.scroll_to(0.0);
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
            Hit::Next | Hit::MiniNext => self.next_track(),
            Hit::Toggle | Hit::MiniToggle => self.toggle_play(),
            Hit::Shuffle => self.player.shuffle = !self.player.shuffle,
            Hit::RepeatBtn => self.player.repeat = self.player.repeat.cycle(),
            Hit::ExpandPlayer => self.open_player(),
            Hit::CollapsePlayer => self.close_player(),
            Hit::EmptyAction => {
                if self.tracks.is_empty() {
                    self.rescan();
                } else {
                    self.scope = Scope::All;
                    self.list_sv.scroll_to(0.0);
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
                let hit = self.hit_at(w, h, pos);
                self.pressed = hit;
                self.seek_drag = None;
                self.list_touch = None;
                // Seek drag captures the touch anywhere in its lane.
                if hit == Some(Hit::Seek) {
                    let lane = self.player_geom(w, h).seek_lane;
                    self.seek_drag = Some((id, Self::seek_fraction(lane, pos)));
                } else if !self.player_open && self.layout(w, h).list.contains(pos) {
                    self.list_touch = Some(id);
                    self.list_sv.on_touch_down(pos.y as f32, self.now_ms());
                }
                dirty = true;
            }
            AppEvent::TouchMove { id, pos } => {
                if let Some((did, _)) = self.seek_drag {
                    if did == id {
                        let lane = self.player_geom(w, h).seek_lane;
                        self.seek_drag = Some((id, Self::seek_fraction(lane, pos)));
                        dirty = true;
                    }
                } else if self.list_touch == Some(id) {
                    if self.list_sv.on_touch_move(pos.y as f32, self.now_ms()) {
                        self.pressed = None; // gesture became a scroll
                    }
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
                } else if self.list_touch == Some(id) {
                    self.list_touch = None;
                    let was_drag = self.list_sv.on_touch_up(self.now_ms());
                    self.pressed = None;
                    if !was_drag {
                        if let Some(hit) = self.hit_at(w, h, pos) {
                            self.dispatch(hit);
                        }
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
            AppEvent::Scroll { dy, .. } => {
                if !self.player_open {
                    self.list_sv.scroll_by(dy);
                    dirty = true;
                }
            }
            AppEvent::Key { utf8: Some(s), .. } if s == " " => {
                self.toggle_play();
                dirty = true;
            }
            AppEvent::Tick(dt) => {
                dirty = true;
                let _ = self.list_sv.tick(dt.as_secs_f32());
                let _ = self.tick_expand(dt.as_secs_f32());
            }
            _ => {}
        }

        AppFrame {
            dirty,
            animating: self.player.has_track()
                || self.list_sv.is_animating()
                || self.expand_anim.is_some(),
            quit: false,
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let changed = self.pump();
        let _ = changed;
        let size = canvas.size();
        let (w, h) = (size.w, size.h);
        self.vp = (w, h);
        let l = self.layout(w, h);
        let content = self.visible().len() as i32 * ROW_H;
        self.list_sv.set_bounds(content as f32, l.list.h as f32);

        // Fully expanded: the player owns the surface, nothing underneath.
        if self.expand_v >= 1.0 {
            self.draw_player(canvas, theme, w, h);
            return;
        }

        self.draw_bar(canvas, theme, &l);
        self.draw_banners(canvas, theme, l.banner_top);
        self.draw_chips(canvas, theme, &l);
        self.draw_section_label(canvas, theme, &l);
        self.draw_list(canvas, theme, &l);
        self.draw_empty(canvas, theme, &l);
        if let Some(np) = l.mini {
            self.draw_mini(canvas, theme, np);
        }

        // Expanding/collapsing: shared-axis fade + vertical translate of the
        // player screen over the library (~300 ms).
        if self.expand_v > 0.0 {
            let mut off = Canvas::new(size);
            self.draw_player(&mut off, theme, w, h);
            let px = off.into_pixels();
            let dy = ((1.0 - self.expand_v) * 160.0).round() as i32;
            canvas.blit_scaled(&px, Rect::new(0, dy, w, h), self.expand_v);
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
    fn seek_fraction_maps_lane_width() {
        let app = MusicApp::new();
        let g = app.player_geom(720, 1600);
        let mid = g.seek_lane.center();
        assert!((MusicApp::seek_fraction(g.seek_lane, mid) - 0.5).abs() < 0.01);
        let left = Point::new(g.seek_lane.x - 40, mid.y);
        assert!(MusicApp::seek_fraction(g.seek_lane, left) < 0.0);
        let right = Point::new(g.seek_lane.right() + 40, mid.y);
        assert!(MusicApp::seek_fraction(g.seek_lane, right) > 1.0);
    }

    #[test]
    fn player_transport_is_five_centered_targets() {
        let app = MusicApp::new();
        let g = app.player_geom(720, 1600);
        let [sh, prev, hero, next, rep] = g.transport;
        // Hero is the biggest, centered horizontally on the surface.
        assert_eq!(hero.w, 96);
        assert_eq!(hero.center().x, 360);
        assert!(sh.w >= 48 && rep.w >= 48 && prev.w >= 48 && next.w >= 48);
        assert!(sh.center().x < prev.center().x && prev.center().x < hero.center().x);
        assert!(hero.center().x < next.center().x && next.center().x < rep.center().x);
        // Everything inside the surface and below the app bar.
        for r in g.transport {
            assert!(r.y >= g.bar.bottom());
            assert!(r.bottom() <= 1600 - ui::BOTTOM_SAFE);
            assert!(r.x >= 0 && r.right() <= 720);
        }
    }

    #[test]
    fn layout_never_hardcodes_panel_size() {
        let app = MusicApp::new();
        for (w, h) in [(720, 1600), (1080, 2400), (540, 960), (320, 640)] {
            let l = app.layout(w, h);
            for button in app.player_geom(w, h).transport {
                assert!(button.x >= 0 && button.right() <= w);
                assert!(button.bottom() <= h - ui::BOTTOM_SAFE);
            }
            assert_eq!(l.list.x, 0);
            assert!(l.list.w <= w);
            assert!(l.list.y < l.list.y + l.list.h);
            if let Some(np) = l.mini {
                assert!(np.y + np.h <= h - ui::BOTTOM_SAFE);
                assert!(np.h <= 112);
            }
        }
    }

    fn fake_tracks(n: usize) -> Vec<Track> {
        (0..n)
            .map(|i| Track {
                path: PathBuf::from(format!(
                    "/m/{}/song_{i}.mp3",
                    if i % 3 == 0 { "Downloads" } else { "Music" }
                )),
                title: ["Aurora", "Breeze", "Copper Pots", "Daybreak"][i % 4].into(),
                ext: "mp3".into(),
                folder: if i % 3 == 0 {
                    crate::library::Folder::Downloads
                } else {
                    crate::library::Folder::Music
                },
                subfolder: "Albums".into(),
                mtime: i as i64,
            })
            .collect()
    }

    #[test]
    fn harness_renders_and_expands_player() {
        let dir = std::path::Path::new("/tmp/rabbiz-shots");
        let mut app = MusicApp::new();
        app.tracks = fake_tracks(24);
        app.player.mpv_ok = true; // hide the install banner for the main shots
        let mut h = rabbiz_app_kit::TestHarness::new(app);
        h.save_png(&dir.join("music-after-library.png")).unwrap();

        // Load a fake track → mini-player docks; seek to a mid position.
        h.state_mut().player.current = Some(1);
        h.state_mut().player.file_path = "/m/Music/song_1.mp3".into();
        h.state_mut().player.duration = 210.0;
        h.state_mut().player.seek_to_fraction(0.42);
        let _ = h.render();
        h.save_png(&dir.join("music-after-mini.png")).unwrap();

        // Tap the mini-player body → fullscreen player.
        let l = h.state().layout(720, 1600);
        let np = l.mini.expect("mini player docked");
        h.tap(Point::new(np.x + np.w / 2, np.center().y));
        assert!(h.state().player_open);
        h.settle(30, std::time::Duration::from_millis(50));
        h.save_png(&dir.join("music-after-player.png")).unwrap();

        // Toggle shuffle + repeat → glyphs re-render selected.
        let g = h.state().player_geom(720, 1600);
        h.tap(g.transport[0].center());
        h.tap(g.transport[4].center());
        assert!(h.state().player.shuffle);
        assert_eq!(h.state().player.repeat, crate::library::Repeat::All);
        h.save_png(&dir.join("music-after-player-active.png"))
            .unwrap();

        // Collapse back to the library.
        h.tap(g.collapse.center());
        h.settle(30, std::time::Duration::from_millis(50));
        assert!(!h.state().player_open);
    }

    #[test]
    fn harness_renders_banners_and_empty_states() {
        let dir = std::path::Path::new("/tmp/rabbiz-shots");
        // mpv missing → honest banner over an empty library.
        let mut app = MusicApp::new();
        app.tracks.clear();
        app.player.mpv_ok = false;
        let mut h = rabbiz_app_kit::TestHarness::new(app);
        h.save_png(&dir.join("music-after-empty.png")).unwrap();

        // Playback error banner over the library + mini player.
        let mut app2 = MusicApp::new();
        app2.tracks = fake_tracks(8);
        app2.player.mpv_ok = false;
        app2.player.error =
            Some("mpv reported a playback error — the file may be unsupported.".into());
        app2.player.current = Some(0);
        app2.player.file_path = "/m/Music/song_0.mp3".into();
        let mut h2 = rabbiz_app_kit::TestHarness::new(app2);
        h2.save_png(&dir.join("music-after-banner.png")).unwrap();
    }
}
