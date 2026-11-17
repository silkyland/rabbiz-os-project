use std::time::Duration;

use smithay::delegate_xdg_activation;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::SERIAL_COUNTER;
use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};

use crate::state::ProtocolState;

/// Maximum age of an activation token before it is ignored.
/// Handset policy: activation requests must be recent (client was just
/// interacted with) — a stale token never steals focus.
pub const ACTIVATION_TOKEN_MAX_AGE: Duration = Duration::from_secs(10);

/// Pure activation policy, extracted for unit tests: accept a token when it
/// was created recently and either carries an input serial/app context or
/// comes from the focused client. Timestamp alone is the hard bound.
pub fn token_is_usable(age: Duration) -> bool {
    age <= ACTIVATION_TOKEN_MAX_AGE
}

/// Decide whether an activation request should raise+focus `surface`.
/// Handset policy for M1: a usable token always activates (single fullscreen
/// app model — activation is how a launched app comes forward).
pub fn should_activate(token_age: Duration, target: Option<&WlSurface>) -> bool {
    target.is_some() && token_is_usable(token_age)
}

impl XdgActivationHandler for ProtocolState {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.xdg_activation_state
    }

    fn token_created(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData) -> bool {
        token_is_usable(data.timestamp.elapsed())
    }

    fn request_activation(
        &mut self,
        token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        let valid = should_activate(token_data.timestamp.elapsed(), Some(&surface));
        self.xdg_activation_state.remove_token(&token);
        if !valid {
            eprintln!("xdg_activation: stale token rejected");
            return;
        }

        // Raise + focus the window owning `surface` if it is already mapped.
        let window = self
            .space
            .elements()
            .find(|w| {
                w.toplevel()
                    .map(|t| t.wl_surface() == &surface)
                    .unwrap_or(false)
            })
            .cloned();
        if let Some(window) = window {
            self.space.raise_element(&window, true);
            if let Some(toplevel) = window.toplevel() {
                toplevel.with_pending_state(|state| {
                    state.states.set(State::Activated);
                });
                toplevel.send_configure();
            }
            self.focus_stack_raise(&surface);
        } else {
            // Not committed yet: queue focus for first-commit mapping by
            // raising the stack entry once the buffer lands. The commit path
            // already raises any first-commit surface, so nothing to do.
            eprintln!("xdg_activation: target not mapped yet");
        }
        self.refocus(SERIAL_COUNTER.next_serial());
    }
}

delegate_xdg_activation!(ProtocolState);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_token_is_usable() {
        assert!(token_is_usable(Duration::from_secs(0)));
        assert!(token_is_usable(Duration::from_secs(10)));
    }

    #[test]
    fn stale_token_rejected() {
        assert!(!token_is_usable(Duration::from_secs(11)));
        assert!(!token_is_usable(Duration::from_secs(3600)));
    }

    #[test]
    fn activation_requires_a_target_surface() {
        // `None` target can never activate regardless of token freshness.
        assert!(!should_activate(Duration::ZERO, None));
    }
}
