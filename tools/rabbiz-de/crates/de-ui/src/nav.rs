//! Shell navigation skeleton (M2-T5): `Screen` enum + a tiny `Nav` stack the
//! compositor drives. Screens are pure render fns; Nav owns which is up.

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
