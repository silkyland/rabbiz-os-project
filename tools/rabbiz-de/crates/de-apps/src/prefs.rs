//! App-level prefs facade (M3-T3): hide/pin live in
//! [`de_config::Prefs`] — this module is the catalog-facing convenience
//! wrapper the shell calls so UI code never touches file paths.

use de_config::Prefs;

/// Loaded prefs + mutation helpers that persist immediately.
///
/// Every mutation saves atomically (see `Prefs::save`) — the app grid can
/// be torn down by a crash mid-gesture and the state is still correct.
#[derive(Debug)]
pub struct AppPrefs {
    prefs: Prefs,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self::load()
    }
}

impl AppPrefs {
    /// Load `~/.config/rabbiz-de/prefs.json` (legacy-seeded on first run).
    #[must_use]
    pub fn load() -> Self {
        Self {
            prefs: Prefs::load(),
        }
    }

    /// Wrap an already-loaded `Prefs` (tests, sim).
    #[must_use]
    pub fn wrap(prefs: Prefs) -> Self {
        Self { prefs }
    }

    /// Read-only view for catalog filters.
    #[must_use]
    pub fn prefs(&self) -> &Prefs {
        &self.prefs
    }

    /// Flip hide state for `desktop_id`, persist, return the new state.
    pub fn toggle_hidden(&mut self, desktop_id: &str) -> bool {
        let now = !self.prefs.is_hidden(desktop_id);
        self.prefs.set_hidden(desktop_id, now);
        let _ = self.prefs.save();
        now
    }

    /// Choose the wallpaper by id; delegates to [`Prefs::set_wallpaper`].
    pub fn set_wallpaper(&mut self, id: &str) -> bool {
        self.prefs.set_wallpaper(id)
    }

    /// Persist DND; delegates to [`Prefs::set_do_not_disturb`].
    pub fn set_do_not_disturb(&mut self, on: bool) -> bool {
        self.prefs.set_do_not_disturb(on)
    }

    /// Select active keyboard id; delegates to [`Prefs::set_keyboard`].
    pub fn set_keyboard(&mut self, id: &str) -> bool {
        self.prefs.set_keyboard(id)
    }

    /// Toggle 3rd-party keyboards; delegates to [`Prefs::set_allow_third_party_keyboards`].
    pub fn set_allow_third_party_keyboards(&mut self, allow: bool) -> bool {
        self.prefs.set_allow_third_party_keyboards(allow)
    }

    /// Pin to dock (no-op when full); persists.
    pub fn pin(&mut self, desktop_id: &str) {
        self.prefs.pin_dock(desktop_id);
        let _ = self.prefs.save();
    }

    /// Unpin from dock; persists.
    pub fn unpin(&mut self, desktop_id: &str) {
        self.prefs.unpin_dock(desktop_id);
        let _ = self.prefs.save();
    }

    /// Dock membership.
    #[must_use]
    pub fn is_pinned(&self, desktop_id: &str) -> bool {
        self.prefs.dock.iter().any(|d| d == desktop_id)
    }

    /// Reorder the home grid: moves `desktop_id` to `index` in
    /// `home_order`, extending the order list to cover all `visible` ids so
    /// the persisted file fully describes the grid.
    pub fn move_on_home(&mut self, desktop_id: &str, index: usize, visible: &[String]) {
        let mut order = self.prefs.home_order.clone();
        for id in visible {
            if !order.contains(id) {
                order.push(id.clone());
            }
        }
        order.retain(|d| d != desktop_id);
        order.insert(index.min(order.len()), desktop_id.into());
        self.prefs.home_order = order;
        let _ = self.prefs.save();
    }

    /// Drag-rearrange: drop on the dock at `slot` (see `Prefs::drop_on_dock`); persists.
    pub fn drop_on_dock(&mut self, desktop_id: &str, slot: usize, swap_to: usize, grid: &[String]) {
        self.prefs.drop_on_dock(desktop_id, slot, swap_to, grid);
        let _ = self.prefs.save();
    }

    /// Drag-rearrange: drop on the grid at `index` (see `Prefs::drop_on_grid`); persists.
    pub fn drop_on_grid(&mut self, desktop_id: &str, index: usize, grid: &[String]) {
        self.prefs.drop_on_grid(desktop_id, index, grid);
        let _ = self.prefs.save();
    }
}
