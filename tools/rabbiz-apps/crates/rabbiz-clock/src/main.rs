use std::time::{Duration, Instant};

use rabbiz_app_kit::{
    ui, AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Clock,
    Stopwatch,
    Timer,
}

struct ClockApp {
    tab: Tab,
    sw_running: bool,
    sw_elapsed: Duration,
    sw_started: Option<Instant>,
    laps: Vec<Duration>,
    tm_total: Duration,
    tm_remaining: Duration,
    tm_running: bool,
    tm_started: Option<Instant>,
    pressed_btn: Option<usize>,
}

impl ClockApp {
    fn new() -> Self {
        let def = Duration::from_secs(300);
        Self {
            tab: Tab::Clock,
            sw_running: false,
            sw_elapsed: Duration::ZERO,
            sw_started: None,
            laps: Vec::new(),
            tm_total: def,
            tm_remaining: def,
            tm_running: false,
            tm_started: None,
            pressed_btn: None,
        }
    }

    fn current_sw(&self) -> Duration {
        self.sw_elapsed + self.sw_started.map_or(Duration::ZERO, |t| t.elapsed())
    }

    fn current_tm(&self) -> Duration {
        if !self.tm_running {
            return self.tm_remaining;
        }
        let used = self.tm_started.map_or(Duration::ZERO, |t| t.elapsed());
        self.tm_remaining.saturating_sub(used)
    }

    fn buttons(&self, w: i32, h: i32) -> Vec<(Rect, &'static str, usize)> {
        let mut out = Vec::new();
        let tab_w = (w - ui::MARGIN * 2 - 16) / 3;
        let tab_y = ui::TOP_SAFE + 86;
        out.push((Rect::new(ui::MARGIN, tab_y, tab_w, 48), "Clock", 0));
        out.push((Rect::new(ui::MARGIN + tab_w + 8, tab_y, tab_w, 48), "Stopwatch", 1));
        out.push((Rect::new(ui::MARGIN + (tab_w + 8) * 2, tab_y, tab_w, 48), "Timer", 2));

        let act_y = h - ui::BOTTOM_SAFE - 80;
        let bw = (w - ui::MARGIN * 2 - 16) / 2;
        match self.tab {
            Tab::Clock => {}
            Tab::Stopwatch => {
                out.push((Rect::new(ui::MARGIN, act_y, bw, 58), if self.sw_running { "Pause" } else { "Start" }, 10));
                out.push((Rect::new(ui::MARGIN + bw + 16, act_y, bw, 58), if self.sw_running { "Lap" } else { "Reset" }, 11));
            }
            Tab::Timer => {
                let cw = (w - ui::MARGIN * 2 - 24) / 3;
                let cy = act_y - 84;
                out.push((Rect::new(ui::MARGIN, cy, cw, 50), "+1m", 20));
                out.push((Rect::new(ui::MARGIN + cw + 12, cy, cw, 50), "+5m", 21));
                out.push((Rect::new(ui::MARGIN + (cw + 12) * 2, cy, cw, 50), "Reset", 22));
                out.push((Rect::new(ui::MARGIN, act_y, w - ui::MARGIN * 2, 58), if self.tm_running { "Pause" } else { "Start" }, 23));
            }
        }
        out
    }
}

impl AppState for ClockApp {
    fn app_id(&self) -> &'static str { "rabbiz-clock" }
    fn title(&self) -> &'static str { "Clock" }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { pos, .. } => {
                let btns = self.buttons(720, 1600);
                self.pressed_btn = btns.iter().find(|(r, _, _)| r.contains(pos)).map(|b| b.2);
                AppFrame { dirty: true, animating: self.sw_running || self.tm_running, quit: false }
            }
            AppEvent::TouchUp { pos, .. } => {
                let btns = self.buttons(720, 1600);
                let hit = btns.iter().find(|(r, _, id)| r.contains(pos) && Some(*id) == self.pressed_btn).map(|b| b.2);
                self.pressed_btn = None;
                match hit {
                    Some(0) => self.tab = Tab::Clock,
                    Some(1) => self.tab = Tab::Stopwatch,
                    Some(2) => self.tab = Tab::Timer,
                    Some(10) => {
                        if self.sw_running {
                            self.sw_elapsed = self.current_sw();
                            self.sw_started = None;
                            self.sw_running = false;
                        } else {
                            self.sw_started = Some(Instant::now());
                            self.sw_running = true;
                        }
                    }
                    Some(11) => {
                        if self.sw_running {
                            let d = self.current_sw();
                            self.laps.insert(0, d);
                        } else {
                            self.sw_elapsed = Duration::ZERO;
                            self.laps.clear();
                        }
                    }
                    Some(20) => {
                        self.tm_total += Duration::from_secs(60);
                        self.tm_remaining += Duration::from_secs(60);
                    }
                    Some(21) => {
                        self.tm_total += Duration::from_secs(300);
                        self.tm_remaining += Duration::from_secs(300);
                    }
                    Some(22) => {
                        self.tm_running = false;
                        self.tm_started = None;
                        self.tm_remaining = self.tm_total;
                    }
                    Some(23) => {
                        if self.tm_running {
                            self.tm_remaining = self.current_tm();
                            self.tm_started = None;
                            self.tm_running = false;
                        } else if self.tm_remaining > Duration::ZERO {
                            self.tm_started = Some(Instant::now());
                            self.tm_running = true;
                        }
                    }
                    _ => {}
                }
                AppFrame { dirty: true, animating: self.sw_running || self.tm_running, quit: false }
            }
            AppEvent::Tick(_) => {
                if self.tm_running && self.current_tm().is_zero() {
                    self.tm_running = false;
                    self.tm_started = None;
                    self.tm_remaining = Duration::ZERO;
                }
                AppFrame { dirty: true, animating: self.sw_running || self.tm_running, quit: false }
            }
            _ => AppFrame { dirty: false, animating: self.sw_running || self.tm_running, quit: false },
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        ui::draw_header(canvas, theme, "Clock", "");
        let btns = self.buttons(w, h);
        for (r, lbl, id) in &btns {
            let active = matches!((self.tab, *id), (Tab::Clock, 0) | (Tab::Stopwatch, 1) | (Tab::Timer, 2)) || *id == 10 || *id == 23;
            ui::draw_pill_button(canvas, theme, *r, lbl, active, self.pressed_btn == Some(*id));
        }

        let card = Rect::new(ui::MARGIN, ui::TOP_SAFE + 164, w - ui::MARGIN * 2, 240);
        ui::draw_card(canvas, theme, card, false);
        match self.tab {
            Tab::Clock => {
                let (hhmm, date) = local_time_strings();
                ui::draw_centered(canvas, &hhmm, &ui::styled(88.0, 600, theme.text), Rect::new(card.x, card.y + 32, card.w, 110));
                ui::draw_centered(canvas, &date, &ui::body(theme.text3), Rect::new(card.x, card.y + 150, card.w, 48));
            }
            Tab::Stopwatch => {
                let d = self.current_sw();
                let s = format!("{:02}:{:02}.{:02}", d.as_secs() / 60, d.as_secs() % 60, (d.subsec_millis() / 10));
                ui::draw_centered(canvas, &s, &ui::styled(72.0, 600, theme.text), card);
                for (i, lap) in self.laps.iter().take(6).enumerate() {
                    let y = card.y + card.h + 20 + i as i32 * 76;
                    let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 64);
                    ui::draw_card(canvas, theme, r, false);
                    let ls = format!("Lap {}   {:02}:{:02}.{:02}", self.laps.len() - i, lap.as_secs() / 60, lap.as_secs() % 60, lap.subsec_millis() / 10);
                    ui::draw_text(canvas, &ls, &ui::body(theme.text), r.x + 24, r.y + 18, Some(r.w - 48));
                }
            }
            Tab::Timer => {
                let d = self.current_tm();
                let s = format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60);
                ui::draw_centered(canvas, &s, &ui::styled(84.0, 600, theme.text), card);
            }
        }
    }
}

fn local_time_strings() -> (String, String) {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        let hhmm = format!("{:02}:{:02}", tm.tm_hour, tm.tm_min);
        let date = format!("{:04}-{:02}-{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday);
        (hhmm, date)
    }
}

fn main() {
    rabbiz_app_kit::run_app(ClockApp::new());
}
