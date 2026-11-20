//! Privacy policy derived from the focused input field's [`InputPurpose`].
//!
//! Sensitive fields (`Password`, `Pin`) disable predictions, dictionary learning,
//! key-press preview popups, and the clipboard suggestion strip so secrets are
//! never echoed on screen or persisted to disk.

pub use crate::state::InputPurpose;

/// Privacy rules governing OSK features for the currently focused field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivacyPolicy {
    sensitive: bool,
}

impl PrivacyPolicy {
    /// Derive the privacy policy from the focused field's [`InputPurpose`].
    pub const fn from_purpose(purpose: InputPurpose) -> Self {
        let sensitive = matches!(purpose, InputPurpose::Password | InputPurpose::Pin);
        Self { sensitive }
    }

    /// Whether the field holds secret/sensitive input (`Password` or `Pin`).
    pub const fn is_sensitive(&self) -> bool {
        self.sensitive
    }

    /// Whether word predictions and completions may be shown in the suggestion strip.
    pub const fn allow_predictions(&self) -> bool {
        !self.sensitive
    }

    /// Whether typed words may be learned into the user dictionary / predictor.
    pub const fn allow_learning(&self) -> bool {
        !self.sensitive
    }

    /// Whether the magnified key-press preview popup may be shown.
    pub const fn allow_key_popup(&self) -> bool {
        !self.sensitive
    }

    /// Whether clipboard paste chips may be shown in the suggestion strip.
    pub const fn allow_clipboard_strip(&self) -> bool {
        !self.sensitive
    }
}

impl Default for PrivacyPolicy {
    fn default() -> Self {
        Self::from_purpose(InputPurpose::Normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_purposes_disable_all_leaky_features() {
        for purpose in [InputPurpose::Password, InputPurpose::Pin] {
            let policy = PrivacyPolicy::from_purpose(purpose);
            assert!(policy.is_sensitive(), "{purpose:?} must be sensitive");
            assert!(
                !policy.allow_predictions(),
                "{purpose:?} must block predictions"
            );
            assert!(!policy.allow_learning(), "{purpose:?} must block learning");
            assert!(
                !policy.allow_key_popup(),
                "{purpose:?} must block key popups"
            );
            assert!(
                !policy.allow_clipboard_strip(),
                "{purpose:?} must block clipboard strip"
            );
        }
    }

    #[test]
    fn non_sensitive_purposes_allow_all_features() {
        let non_sensitive = [
            InputPurpose::Normal,
            InputPurpose::Alpha,
            InputPurpose::Digits,
            InputPurpose::Number,
            InputPurpose::Phone,
            InputPurpose::Url,
            InputPurpose::Email,
            InputPurpose::Name,
            InputPurpose::Date,
            InputPurpose::Time,
            InputPurpose::Terminal,
        ];
        for purpose in non_sensitive {
            let policy = PrivacyPolicy::from_purpose(purpose);
            assert!(!policy.is_sensitive(), "{purpose:?} must not be sensitive");
            assert!(policy.allow_predictions());
            assert!(policy.allow_learning());
            assert!(policy.allow_key_popup());
            assert!(policy.allow_clipboard_strip());
        }
    }

    #[test]
    fn default_policy_matches_normal_purpose() {
        assert_eq!(
            PrivacyPolicy::default(),
            PrivacyPolicy::from_purpose(InputPurpose::Normal)
        );
    }
}
