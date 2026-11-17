//! Direct-scanout observation for the DRM compositor (M1-T4).
//!
//! The counters are observational, not predictive: they record what
//! `render_frame`'s per-element assignment actually did on the primary plane.

use smithay::backend::allocator::Buffer;
use smithay::backend::drm::compositor::PrimaryPlaneElement;
use smithay::backend::drm::Framebuffer;
use smithay::backend::renderer::element::{RenderElementPresentationState, RenderElementStates};

/// Cumulative presentation counters for the stats stream.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScanoutCounters {
    /// Frames where a client element scanned out directly on the primary
    /// plane (zero-copy).
    pub direct_scanout: u64,
    /// Frames the primary plane showed the composited swapchain.
    pub primary_composite: u64,
}

/// What the primary plane carried in a submitted frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryOutcome {
    /// One element was assigned to the primary plane (zero-copy).
    DirectScanout,
    /// The primary plane showed the composited swapchain.
    Composite,
}

/// Classify `Frame::primary_element` for the counters.
pub fn classify_primary<B: Buffer, F: Framebuffer, E>(
    primary: &PrimaryPlaneElement<'_, B, F, E>,
) -> PrimaryOutcome {
    match primary {
        PrimaryPlaneElement::Element(_) => PrimaryOutcome::DirectScanout,
        PrimaryPlaneElement::Swapchain(_) => PrimaryOutcome::Composite,
    }
}

/// Record one submitted frame's primary-plane outcome.
pub fn record<B: Buffer, F: Framebuffer, E>(
    primary: &PrimaryPlaneElement<'_, B, F, E>,
    counters: &mut ScanoutCounters,
) -> PrimaryOutcome {
    let outcome = classify_primary(primary);
    match outcome {
        PrimaryOutcome::DirectScanout => counters.direct_scanout += 1,
        PrimaryOutcome::Composite => counters.primary_composite += 1,
    }
    outcome
}

/// Compact per-element report (`id → presentation[/reason]`) — the evidence
/// line for the S1 scanout waiver when `direct_scanout` stays 0.
pub fn summarize_states(states: &RenderElementStates) -> String {
    states
        .states
        .iter()
        .map(|(id, st)| {
            let detail = match st.presentation_state {
                RenderElementPresentationState::ZeroCopy => "zero-copy".to_string(),
                RenderElementPresentationState::Skipped => "skipped".to_string(),
                RenderElementPresentationState::Rendering { reason } => match reason {
                    Some(r) => format!("rendering({r:?})"),
                    None => "rendering".to_string(),
                },
            };
            format!("{:?}:{detail}", id)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::backend::renderer::element::{Id, RenderElementState};

    fn state(p: RenderElementPresentationState) -> RenderElementState {
        RenderElementState {
            visible_area: 1,
            presentation_state: p,
        }
    }

    #[test]
    fn summarize_names_each_presentation_state() {
        let mut states = RenderElementStates::default();
        let a = Id::new();
        let b = Id::new();
        let c = Id::new();
        states.states.insert(
            a.clone(),
            state(RenderElementPresentationState::ZeroCopy),
        );
        states.states.insert(
            b.clone(),
            state(RenderElementPresentationState::Skipped),
        );
        states.states.insert(
            c.clone(),
            state(RenderElementPresentationState::Rendering { reason: None }),
        );
        let out = summarize_states(&states);
        assert!(out.contains("zero-copy"), "missing zero-copy: {out}");
        assert!(out.contains("skipped"), "missing skipped: {out}");
        assert!(out.contains("rendering"), "missing rendering: {out}");
    }
}
