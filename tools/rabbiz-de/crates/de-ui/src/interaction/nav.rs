//! Shell navigation skeleton (M2-T5): `Screen` enum + a tiny `Nav` stack the
//! compositor drives. Screens are pure render fns; Nav owns which is up.

use de_design::motion::Transition;

/// All P-screens the shell can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Boot,
    Lock,
    Home,
    Drawer,
    Recents,
    Shade,
    QuickSettings,
    Osd,
    Power,
    Search,
    Screenshot,
    Error,
    FirstRun,
    Settings,
}

impl Screen {
    /// Default motion policy for this surface. Keep this match exhaustive so a
    /// new navigable screen must have a transition in the design catalogue.
    #[must_use]
    pub const fn transition(self) -> Transition {
        match self {
            Self::Boot => Transition::Boot,
            Self::Lock => Transition::Lock,
            Self::Home => Transition::Home,
            Self::Drawer => Transition::Drawer,
            Self::Recents => Transition::Recents,
            Self::Shade => Transition::Shade,
            Self::QuickSettings => Transition::QuickSettings,
            Self::Osd => Transition::Osd,
            Self::Power => Transition::Power,
            Self::Search => Transition::Search,
            Self::Screenshot => Transition::Screenshot,
            Self::Error => Transition::Error,
            Self::FirstRun => Transition::FirstRun,
            Self::Settings => Transition::Settings,
        }
    }
}

/// Back-stack navigation: `current` + up to 8-deep stack.
#[derive(Debug)]
pub struct Nav {
    pub current: Screen,
    stack: Vec<Screen>,
}

impl Default for Nav {
    fn default() -> Self {
        Self {
            current: Screen::Boot,
            stack: Vec::new(),
        }
    }
}

impl Nav {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push `s` (records `current` on the stack).
    pub fn go(&mut self, s: Screen) {
        if self.stack.len() < 8 {
            self.stack.push(self.current);
        }
        self.current = s;
    }

    /// Pop the stack; returns the screen now current.
    pub fn back(&mut self) -> Screen {
        if let Some(p) = self.stack.pop() {
            self.current = p;
        }
        self.current
    }

    /// Boot → Lock once the compositor is up.
    pub fn booted(&mut self) {
        self.current = Screen::Lock;
        self.stack.clear();
    }

    /// Lock → Home on unlock.
    pub fn unlock(&mut self) {
        self.current = Screen::Home;
        self.stack.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_design::motion::SpeedTier::{Default, Fast, Slow};

    #[test]
    fn every_screen_has_a_reference_tier() {
        for (screen, tier) in [
            (Screen::Boot, Slow),
            (Screen::Lock, Slow),
            (Screen::Home, Slow),
            (Screen::Drawer, Default),
            (Screen::Recents, Slow),
            (Screen::Shade, Default),
            (Screen::QuickSettings, Default),
            (Screen::Osd, Fast),
            (Screen::Power, Default),
            (Screen::Search, Slow),
            (Screen::Screenshot, Fast),
            (Screen::Error, Default),
            (Screen::FirstRun, Slow),
            (Screen::Settings, Slow),
        ] {
            assert_eq!(screen.transition().timing().tier, tier, "{screen:?}");
        }
    }

    #[test]
    fn boot_lock_home_flow() {
        let mut n = Nav::new();
        assert_eq!(n.current, Screen::Boot);
        n.booted();
        assert_eq!(n.current, Screen::Lock);
        n.unlock();
        assert_eq!(n.current, Screen::Home);
    }

    #[test]
    fn back_restores() {
        let mut n = Nav::new();
        n.booted();
        n.unlock();
        n.go(Screen::Settings);
        n.go(Screen::Search);
        assert_eq!(n.back(), Screen::Settings);
        assert_eq!(n.back(), Screen::Home);
        assert_eq!(n.back(), Screen::Home); // empty stack stays
    }
}
