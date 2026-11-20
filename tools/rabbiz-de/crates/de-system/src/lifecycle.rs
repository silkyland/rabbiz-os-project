//! Pure, host-testable mobile app lifecycle state machine and cgroup v2 policy planner.
//!
//! Implements Phase 1 of `docs/DE-APP-LIFECYCLE.md`:
//! - 5-state lifecycle (`Starting`, `Foreground`, `Background`, `Frozen`, `Dead`)
//! - Per-state `CgroupPolicy` (`oom_score_adj`, `CPUWeight`, `MemoryHigh`, `cgroup.freeze`)
//! - Freeze exemptions (audio/MPRIS, active call, idle inhibitor, user pinned)
//! - Grace-period freeze transitions and LRU memory-pressure eviction
//! - Safe systemd transient `.scope` unit name sanitization

use std::collections::BTreeMap;

/// Default grace period (in milliseconds) before a non-exempt `Background` app is frozen.
pub const DEFAULT_FREEZE_GRACE_MS: u64 = 15_000;

/// Maximum length of the sanitized app-id segment inside a systemd `.scope` unit name.
const MAX_SANITIZED_APP_ID_LEN: usize = 64;

/// Lifecycle state of a managed application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppState {
    /// Process/scope spawned, waiting for the first `xdg_toplevel` to map.
    Starting,
    /// Currently focused and visible to the user.
    Foreground,
    /// Off-screen / unfocused, still runnable during the freeze grace period (or while exempt).
    Background,
    /// Suspended via cgroup v2 `cgroup.freeze = 1`.
    Frozen,
    /// Exited, dismissed, or evicted under memory pressure.
    Dead,
}

/// Conditions that exempt a `Background` application from being frozen or evicted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FreezeExemptions {
    /// Active PipeWire audio playback or MPRIS `Playing` status.
    pub active_audio: bool,
    /// Active voice call or modem session (e.g., dialer).
    pub active_call: bool,
    /// Surface holds a Wayland `zwp_idle_inhibitor_v1`.
    pub idle_inhibited: bool,
    /// Explicitly pinned or opted-in by the user/config.
    pub user_pinned: bool,
}

impl FreezeExemptions {
    /// Returns `true` if any exemption is currently active.
    #[must_use]
    pub const fn is_exempt(&self) -> bool {
        self.active_audio || self.active_call || self.idle_inhibited || self.user_pinned
    }
}

/// Resource and OOM policy applied to an application's systemd cgroup v2 scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CgroupPolicy {
    /// Kernel OOM score adjustment (`-1000..=1000`).
    pub oom_score_adj: i16,
    /// systemd `CPUWeight` (`1..=10000`).
    pub cpu_weight: u16,
    /// Soft memory cap as a percentage of usable RAM (`MemoryHigh`).
    pub memory_high_pct: u8,
    /// Whether `cgroup.freeze` should be set to `1`.
    pub frozen: bool,
}

impl CgroupPolicy {
    /// Returns the cgroup/OOM policy for the given [`AppState`], matching
    /// `docs/DE-APP-LIFECYCLE.md`.
    #[must_use]
    pub const fn for_state(state: AppState) -> Self {
        match state {
            AppState::Starting | AppState::Foreground => Self {
                oom_score_adj: 100,
                cpu_weight: 150,
                memory_high_pct: 70,
                frozen: false,
            },
            AppState::Background => Self {
                oom_score_adj: 400,
                cpu_weight: 50,
                memory_high_pct: 40,
                frozen: false,
            },
            AppState::Frozen => Self {
                oom_score_adj: 700,
                cpu_weight: 10,
                memory_high_pct: 25,
                frozen: true,
            },
            AppState::Dead => Self {
                oom_score_adj: 1000,
                cpu_weight: 1,
                memory_high_pct: 0,
                frozen: false,
            },
        }
    }
}

/// Side-effect requested by [`LifecycleManager`] for the system backend to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleAction {
    /// Apply the given [`CgroupPolicy`] to the application's transient scope.
    ApplyPolicy {
        app_id: String,
        scope: String,
        policy: CgroupPolicy,
    },
    /// Evict (unfreeze if needed and terminate) the application's transient scope.
    Evict { app_id: String, scope: String },
}

/// Tracked lifecycle record for a single application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRecord {
    pub app_id: String,
    pub scope_unit: String,
    pub state: AppState,
    pub exemptions: FreezeExemptions,
    pub entered_state_at_ms: u64,
    pub last_focused_at_ms: u64,
}

/// Pure state machine managing application lifecycles, freeze timers, and LRU eviction.
#[derive(Debug, Clone)]
pub struct LifecycleManager {
    freeze_grace_ms: u64,
    next_instance_id: u64,
    apps: BTreeMap<String, AppRecord>,
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new(DEFAULT_FREEZE_GRACE_MS)
    }
}

impl LifecycleManager {
    /// Creates a new [`LifecycleManager`] with the given background freeze grace period.
    #[must_use]
    pub const fn new(freeze_grace_ms: u64) -> Self {
        Self {
            freeze_grace_ms,
            next_instance_id: 1,
            apps: BTreeMap::new(),
        }
    }

    /// Returns the configured freeze grace period in milliseconds.
    #[must_use]
    pub const fn freeze_grace_ms(&self) -> u64 {
        self.freeze_grace_ms
    }

    /// Formats a safe systemd transient scope unit name:
    /// `app-rabbiz-<sanitized>-<instance_id>.scope`.
    ///
    /// Keeps only ASCII alphanumerics, `-`, and `_`; replaces runs of other characters
    /// with a single `-`, trims leading/trailing `-`, bounds the sanitized segment length,
    /// and falls back to `"unknown"` if nothing remains.
    #[must_use]
    pub fn sanitize_scope_unit(app_id: &str, instance_id: u64) -> String {
        let mut sanitized = String::with_capacity(app_id.len().min(MAX_SANITIZED_APP_ID_LEN));
        let mut last_was_dash = false;

        for ch in app_id.chars() {
            if sanitized.len() >= MAX_SANITIZED_APP_ID_LEN {
                break;
            }
            if ch.is_ascii_alphanumeric() || ch == '_' {
                sanitized.push(ch);
                last_was_dash = false;
            } else if !last_was_dash && !sanitized.is_empty() {
                sanitized.push('-');
                last_was_dash = true;
            }
        }

        while sanitized.ends_with('-') {
            sanitized.pop();
        }

        if sanitized.is_empty() {
            sanitized.push_str("unknown");
        }

        format!("app-rabbiz-{sanitized}-{instance_id}.scope")
    }

    /// Records a newly launched application in `Starting` state and emits its initial policy.
    pub fn on_launch(&mut self, app_id: &str, now_ms: u64) -> Vec<LifecycleAction> {
        let instance_id = self.next_instance_id;
        self.next_instance_id = self.next_instance_id.saturating_add(1);

        let scope_unit = Self::sanitize_scope_unit(app_id, instance_id);
        let record = AppRecord {
            app_id: app_id.to_owned(),
            scope_unit: scope_unit.clone(),
            state: AppState::Starting,
            exemptions: FreezeExemptions::default(),
            entered_state_at_ms: now_ms,
            last_focused_at_ms: now_ms,
        };
        self.apps.insert(app_id.to_owned(), record);

        vec![LifecycleAction::ApplyPolicy {
            app_id: app_id.to_owned(),
            scope: scope_unit,
            policy: CgroupPolicy::for_state(AppState::Starting),
        }]
    }

    /// Transitions a `Starting` application to `Foreground` when its first `xdg_toplevel` maps.
    ///
    /// Any previously `Foreground` application is demoted to `Background`.
    pub fn on_mapped(&mut self, app_id: &str, now_ms: u64) -> Vec<LifecycleAction> {
        let is_starting = self
            .apps
            .get(app_id)
            .is_some_and(|rec| rec.state == AppState::Starting);
        if !is_starting {
            return Vec::new();
        }

        let mut actions = self.demote_foreground(Some(app_id), now_ms);

        if let Some(rec) = self.apps.get_mut(app_id) {
            rec.state = AppState::Foreground;
            rec.entered_state_at_ms = now_ms;
            rec.last_focused_at_ms = now_ms;
            actions.push(LifecycleAction::ApplyPolicy {
                app_id: rec.app_id.clone(),
                scope: rec.scope_unit.clone(),
                policy: CgroupPolicy::for_state(AppState::Foreground),
            });
        }

        actions
    }

    /// Updates focus to `app_id` (or `None` when returning to Home / no app focused).
    ///
    /// Demotes the previous `Foreground` app to `Background`. If the focused target is
    /// `Frozen`, it is thawed (`frozen = false`) as part of transitioning to `Foreground`.
    pub fn on_focus(&mut self, app_id: Option<&str>, now_ms: u64) -> Vec<LifecycleAction> {
        if let Some(target_id) = app_id {
            if let Some(rec) = self.apps.get_mut(target_id) {
                if rec.state == AppState::Foreground {
                    rec.last_focused_at_ms = now_ms;
                    return Vec::new();
                }
            }
        }

        let mut actions = self.demote_foreground(app_id, now_ms);

        let Some(target_id) = app_id else {
            return actions;
        };

        if let Some(rec) = self.apps.get_mut(target_id) {
            if rec.state == AppState::Dead {
                return actions;
            }
            if rec.state == AppState::Frozen {
                // Unfreeze before applying full Foreground policy / input delivery.
                let mut thaw_policy = CgroupPolicy::for_state(AppState::Frozen);
                thaw_policy.frozen = false;
                actions.push(LifecycleAction::ApplyPolicy {
                    app_id: rec.app_id.clone(),
                    scope: rec.scope_unit.clone(),
                    policy: thaw_policy,
                });
            }
            rec.state = AppState::Foreground;
            rec.entered_state_at_ms = now_ms;
            rec.last_focused_at_ms = now_ms;
            actions.push(LifecycleAction::ApplyPolicy {
                app_id: rec.app_id.clone(),
                scope: rec.scope_unit.clone(),
                policy: CgroupPolicy::for_state(AppState::Foreground),
            });
        }

        actions
    }

    /// Updates freeze exemptions for `app_id`.
    ///
    /// If the app is currently `Frozen` and becomes exempt, it is immediately thawed to
    /// `Background`. If a `Background` app loses all exemptions, its freeze grace timer
    /// restarts from `now_ms`.
    pub fn set_exemptions(
        &mut self,
        app_id: &str,
        exemptions: FreezeExemptions,
        now_ms: u64,
    ) -> Vec<LifecycleAction> {
        let Some(rec) = self.apps.get_mut(app_id) else {
            return Vec::new();
        };
        if rec.state == AppState::Dead {
            return Vec::new();
        }

        let was_exempt = rec.exemptions.is_exempt();
        let now_exempt = exemptions.is_exempt();
        rec.exemptions = exemptions;

        if rec.state == AppState::Frozen && now_exempt {
            rec.state = AppState::Background;
            rec.entered_state_at_ms = now_ms;
            return vec![LifecycleAction::ApplyPolicy {
                app_id: rec.app_id.clone(),
                scope: rec.scope_unit.clone(),
                policy: CgroupPolicy::for_state(AppState::Background),
            }];
        }

        if rec.state == AppState::Background && was_exempt && !now_exempt {
            rec.entered_state_at_ms = now_ms;
        }

        Vec::new()
    }

    /// Advances timers and freezes any non-exempt `Background` apps whose grace period has elapsed.
    pub fn tick(&mut self, now_ms: u64) -> Vec<LifecycleAction> {
        let mut actions = Vec::new();

        for rec in self.apps.values_mut() {
            if rec.state == AppState::Background
                && !rec.exemptions.is_exempt()
                && now_ms.saturating_sub(rec.entered_state_at_ms) >= self.freeze_grace_ms
            {
                rec.state = AppState::Frozen;
                rec.entered_state_at_ms = now_ms;
                actions.push(LifecycleAction::ApplyPolicy {
                    app_id: rec.app_id.clone(),
                    scope: rec.scope_unit.clone(),
                    policy: CgroupPolicy::for_state(AppState::Frozen),
                });
            }
        }

        actions
    }

    /// Selects and evicts one victim application under memory pressure using LRU order.
    ///
    /// Prefers the least-recently-focused `Frozen` app first, then the least-recently-focused
    /// non-exempt `Background` app. Never evicts `Foreground`, `Starting`, `Dead`, or exempt apps.
    pub fn on_memory_pressure(&mut self, now_ms: u64) -> Option<LifecycleAction> {
        let victim_id = self
            .select_lru_victim(AppState::Frozen)
            .or_else(|| self.select_lru_victim(AppState::Background))?;

        let rec = self.apps.get_mut(&victim_id)?;
        rec.state = AppState::Dead;
        rec.entered_state_at_ms = now_ms;

        Some(LifecycleAction::Evict {
            app_id: rec.app_id.clone(),
            scope: rec.scope_unit.clone(),
        })
    }

    /// Marks `app_id` as `Dead` after its process/scope exits.
    pub fn on_exited(&mut self, app_id: &str) {
        if let Some(rec) = self.apps.get_mut(app_id) {
            rec.state = AppState::Dead;
        }
    }

    /// Returns the current [`AppState`] of `app_id`, if tracked.
    #[must_use]
    pub fn state_of(&self, app_id: &str) -> Option<AppState> {
        self.apps.get(app_id).map(|rec| rec.state)
    }

    /// Returns the full [`AppRecord`] for `app_id`, if tracked.
    #[must_use]
    pub fn record(&self, app_id: &str) -> Option<&AppRecord> {
        self.apps.get(app_id)
    }

    /// Iterates over all tracked [`AppRecord`] entries in deterministic `app_id` order.
    pub fn records(&self) -> impl Iterator<Item = &AppRecord> {
        self.apps.values()
    }

    fn demote_foreground(
        &mut self,
        except_app_id: Option<&str>,
        now_ms: u64,
    ) -> Vec<LifecycleAction> {
        let mut actions = Vec::new();
        for rec in self.apps.values_mut() {
            if rec.state == AppState::Foreground && Some(rec.app_id.as_str()) != except_app_id {
                rec.state = AppState::Background;
                rec.entered_state_at_ms = now_ms;
                actions.push(LifecycleAction::ApplyPolicy {
                    app_id: rec.app_id.clone(),
                    scope: rec.scope_unit.clone(),
                    policy: CgroupPolicy::for_state(AppState::Background),
                });
            }
        }
        actions
    }

    fn select_lru_victim(&self, required_state: AppState) -> Option<String> {
        self.apps
            .values()
            .filter(|rec| rec.state == required_state && !rec.exemptions.is_exempt())
            .min_by(|a, b| {
                (a.last_focused_at_ms, a.entered_state_at_ms, &a.app_id).cmp(&(
                    b.last_focused_at_ms,
                    b.entered_state_at_ms,
                    &b.app_id,
                ))
            })
            .map(|rec| rec.app_id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cgroup_policy_matches_spec_table() {
        let starting = CgroupPolicy::for_state(AppState::Starting);
        assert_eq!(starting.oom_score_adj, 100);
        assert_eq!(starting.cpu_weight, 150);
        assert_eq!(starting.memory_high_pct, 70);
        assert!(!starting.frozen);

        let fg = CgroupPolicy::for_state(AppState::Foreground);
        assert_eq!(fg, starting);

        let bg = CgroupPolicy::for_state(AppState::Background);
        assert_eq!(bg.oom_score_adj, 400);
        assert_eq!(bg.cpu_weight, 50);
        assert_eq!(bg.memory_high_pct, 40);
        assert!(!bg.frozen);

        let frozen = CgroupPolicy::for_state(AppState::Frozen);
        assert_eq!(frozen.oom_score_adj, 700);
        assert_eq!(frozen.cpu_weight, 10);
        assert_eq!(frozen.memory_high_pct, 25);
        assert!(frozen.frozen);

        let dead = CgroupPolicy::for_state(AppState::Dead);
        assert!(!dead.frozen);
    }

    #[test]
    fn full_lifecycle_transitions_starting_fg_bg_frozen_fg_dead() {
        let mut mgr = LifecycleManager::new(15_000);

        // Launch -> Starting
        let actions = mgr.on_launch("rabbiz-notes", 1_000);
        assert_eq!(mgr.state_of("rabbiz-notes"), Some(AppState::Starting));
        assert_eq!(
            actions,
            vec![LifecycleAction::ApplyPolicy {
                app_id: "rabbiz-notes".into(),
                scope: "app-rabbiz-rabbiz-notes-1.scope".into(),
                policy: CgroupPolicy::for_state(AppState::Starting),
            }]
        );

        // Mapped -> Foreground
        let actions = mgr.on_mapped("rabbiz-notes", 1_500);
        assert_eq!(mgr.state_of("rabbiz-notes"), Some(AppState::Foreground));
        assert_eq!(
            actions,
            vec![LifecycleAction::ApplyPolicy {
                app_id: "rabbiz-notes".into(),
                scope: "app-rabbiz-rabbiz-notes-1.scope".into(),
                policy: CgroupPolicy::for_state(AppState::Foreground),
            }]
        );

        // Home pressed -> Background
        let actions = mgr.on_focus(None, 5_000);
        assert_eq!(mgr.state_of("rabbiz-notes"), Some(AppState::Background));
        assert_eq!(
            actions,
            vec![LifecycleAction::ApplyPolicy {
                app_id: "rabbiz-notes".into(),
                scope: "app-rabbiz-rabbiz-notes-1.scope".into(),
                policy: CgroupPolicy::for_state(AppState::Background),
            }]
        );

        // Tick before grace period expires -> still Background
        assert!(mgr.tick(19_999).is_empty());
        assert_eq!(mgr.state_of("rabbiz-notes"), Some(AppState::Background));

        // Tick at grace period boundary (5_000 + 15_000) -> Frozen
        let actions = mgr.tick(20_000);
        assert_eq!(mgr.state_of("rabbiz-notes"), Some(AppState::Frozen));
        assert_eq!(
            actions,
            vec![LifecycleAction::ApplyPolicy {
                app_id: "rabbiz-notes".into(),
                scope: "app-rabbiz-rabbiz-notes-1.scope".into(),
                policy: CgroupPolicy::for_state(AppState::Frozen),
            }]
        );

        // Refocus Frozen app -> thaws first, then applies Foreground policy
        let actions = mgr.on_focus(Some("rabbiz-notes"), 30_000);
        assert_eq!(mgr.state_of("rabbiz-notes"), Some(AppState::Foreground));
        assert_eq!(actions.len(), 2);
        match &actions[0] {
            LifecycleAction::ApplyPolicy { app_id, policy, .. } => {
                assert_eq!(app_id, "rabbiz-notes");
                assert!(!policy.frozen, "first action must thaw the frozen scope");
            }
            LifecycleAction::Evict { .. } => panic!("expected thaw ApplyPolicy"),
        }
        assert_eq!(
            actions[1],
            LifecycleAction::ApplyPolicy {
                app_id: "rabbiz-notes".into(),
                scope: "app-rabbiz-rabbiz-notes-1.scope".into(),
                policy: CgroupPolicy::for_state(AppState::Foreground),
            }
        );

        // Exit -> Dead
        mgr.on_exited("rabbiz-notes");
        assert_eq!(mgr.state_of("rabbiz-notes"), Some(AppState::Dead));
    }

    #[test]
    fn focus_switch_demotes_previous_foreground_and_mapping_demotes_previous() {
        let mut mgr = LifecycleManager::default();
        assert_eq!(mgr.freeze_grace_ms(), DEFAULT_FREEZE_GRACE_MS);

        mgr.on_launch("app-a", 100);
        mgr.on_mapped("app-a", 200);
        assert_eq!(mgr.state_of("app-a"), Some(AppState::Foreground));

        // Launching + mapping app-b demotes app-a to Background
        mgr.on_launch("app-b", 300);
        let actions = mgr.on_mapped("app-b", 400);
        assert_eq!(mgr.state_of("app-a"), Some(AppState::Background));
        assert_eq!(mgr.state_of("app-b"), Some(AppState::Foreground));
        assert_eq!(actions.len(), 2);

        // Focusing app-a demotes app-b and promotes app-a
        let actions = mgr.on_focus(Some("app-a"), 500);
        assert_eq!(mgr.state_of("app-a"), Some(AppState::Foreground));
        assert_eq!(mgr.state_of("app-b"), Some(AppState::Background));
        assert_eq!(actions.len(), 2);

        // Re-focusing already-foreground app-a is a no-op for policies
        assert!(mgr.on_focus(Some("app-a"), 600).is_empty());
        assert_eq!(mgr.record("app-a").unwrap().last_focused_at_ms, 600);
    }

    #[test]
    fn exemptions_prevent_freeze_for_each_exemption_kind() {
        let cases = [
            FreezeExemptions {
                active_audio: true,
                ..Default::default()
            },
            FreezeExemptions {
                active_call: true,
                ..Default::default()
            },
            FreezeExemptions {
                idle_inhibited: true,
                ..Default::default()
            },
            FreezeExemptions {
                user_pinned: true,
                ..Default::default()
            },
        ];

        for (idx, exemption) in cases.into_iter().enumerate() {
            assert!(exemption.is_exempt());
            let mut mgr = LifecycleManager::new(15_000);
            let app = format!("app-{idx}");
            mgr.on_launch(&app, 0);
            mgr.on_mapped(&app, 100);
            mgr.on_focus(None, 1_000);
            mgr.set_exemptions(&app, exemption, 1_100);

            // Well past grace period, app stays Background
            assert!(mgr.tick(60_000).is_empty());
            assert_eq!(mgr.state_of(&app), Some(AppState::Background));

            // Clearing exemption restarts grace timer and allows freeze after 15 s
            assert!(mgr
                .set_exemptions(&app, FreezeExemptions::default(), 60_000)
                .is_empty());
            assert!(mgr.tick(74_999).is_empty());
            assert_eq!(mgr.tick(75_000).len(), 1);
            assert_eq!(mgr.state_of(&app), Some(AppState::Frozen));
        }
    }

    #[test]
    fn gaining_exemption_thaws_frozen_app_to_background() {
        let mut mgr = LifecycleManager::new(15_000);
        mgr.on_launch("rabbiz-dialer", 0);
        mgr.on_mapped("rabbiz-dialer", 100);
        mgr.on_focus(None, 1_000);
        assert_eq!(mgr.tick(16_000).len(), 1);
        assert_eq!(mgr.state_of("rabbiz-dialer"), Some(AppState::Frozen));

        // Incoming call grants exemption -> immediately thaws to Background
        let actions = mgr.set_exemptions(
            "rabbiz-dialer",
            FreezeExemptions {
                active_call: true,
                ..Default::default()
            },
            20_000,
        );
        assert_eq!(mgr.state_of("rabbiz-dialer"), Some(AppState::Background));
        assert_eq!(
            actions,
            vec![LifecycleAction::ApplyPolicy {
                app_id: "rabbiz-dialer".into(),
                scope: "app-rabbiz-rabbiz-dialer-1.scope".into(),
                policy: CgroupPolicy::for_state(AppState::Background),
            }]
        );
    }

    #[test]
    fn memory_pressure_lru_evicts_oldest_frozen_then_bg_and_protects_fg_starting_exempt() {
        let mut mgr = LifecycleManager::new(15_000);

        // Launch and map 5 apps in sequence:
        // app-1: oldest Frozen (focused at 1_000)
        // app-2: newer Frozen (focused at 2_000)
        // app-3: exempt Background (active audio, focused at 3_000)
        // app-4: non-exempt Background (not yet frozen, focused at 25_000)
        // app-5: Foreground (focused at 26_000)
        // app-6: Starting (launched at 27_000)
        mgr.on_launch("app-1", 900);
        mgr.on_mapped("app-1", 1_000);

        mgr.on_launch("app-2", 1_900);
        mgr.on_mapped("app-2", 2_000);

        mgr.on_launch("app-3", 2_900);
        mgr.on_mapped("app-3", 3_000);
        mgr.set_exemptions(
            "app-3",
            FreezeExemptions {
                active_audio: true,
                ..Default::default()
            },
            3_100,
        );

        mgr.on_focus(None, 4_000);
        // Freeze app-1 and app-2 (app-3 is exempt)
        let frozen_actions = mgr.tick(20_000);
        assert_eq!(frozen_actions.len(), 2);
        assert_eq!(mgr.state_of("app-1"), Some(AppState::Frozen));
        assert_eq!(mgr.state_of("app-2"), Some(AppState::Frozen));
        assert_eq!(mgr.state_of("app-3"), Some(AppState::Background));

        mgr.on_launch("app-4", 24_900);
        mgr.on_mapped("app-4", 25_000);

        mgr.on_launch("app-5", 25_900);
        mgr.on_mapped("app-5", 26_000);

        mgr.on_launch("app-6", 27_000);

        assert_eq!(mgr.state_of("app-4"), Some(AppState::Background));
        assert_eq!(mgr.state_of("app-5"), Some(AppState::Foreground));
        assert_eq!(mgr.state_of("app-6"), Some(AppState::Starting));

        // 1st eviction -> oldest Frozen (app-1)
        let evict1 = mgr.on_memory_pressure(28_000);
        assert_eq!(
            evict1,
            Some(LifecycleAction::Evict {
                app_id: "app-1".into(),
                scope: "app-rabbiz-app-1-1.scope".into(),
            })
        );
        assert_eq!(mgr.state_of("app-1"), Some(AppState::Dead));

        // 2nd eviction -> next oldest Frozen (app-2)
        let evict2 = mgr.on_memory_pressure(28_100);
        assert_eq!(
            evict2,
            Some(LifecycleAction::Evict {
                app_id: "app-2".into(),
                scope: "app-rabbiz-app-2-2.scope".into(),
            })
        );
        assert_eq!(mgr.state_of("app-2"), Some(AppState::Dead));

        // 3rd eviction -> no Frozen left, picks non-exempt Background (app-4), skips exempt app-3
        let evict3 = mgr.on_memory_pressure(28_200);
        assert_eq!(
            evict3,
            Some(LifecycleAction::Evict {
                app_id: "app-4".into(),
                scope: "app-rabbiz-app-4-4.scope".into(),
            })
        );
        assert_eq!(mgr.state_of("app-4"), Some(AppState::Dead));

        // 4th eviction -> only exempt Background (app-3), Foreground (app-5), and Starting (app-6) remain -> None
        assert_eq!(mgr.on_memory_pressure(28_300), None);
        assert_eq!(mgr.state_of("app-3"), Some(AppState::Background));
        assert_eq!(mgr.state_of("app-5"), Some(AppState::Foreground));
        assert_eq!(mgr.state_of("app-6"), Some(AppState::Starting));
    }

    #[test]
    fn sanitize_scope_unit_handles_edge_cases() {
        assert_eq!(
            LifecycleManager::sanitize_scope_unit("org.kde.kalk", 1),
            "app-rabbiz-org-kde-kalk-1.scope"
        );
        assert_eq!(
            LifecycleManager::sanitize_scope_unit("My Cool App!", 2),
            "app-rabbiz-My-Cool-App-2.scope"
        );
        assert_eq!(
            LifecycleManager::sanitize_scope_unit("foo_bar-baz", 3),
            "app-rabbiz-foo_bar-baz-3.scope"
        );
        assert_eq!(
            LifecycleManager::sanitize_scope_unit("", 4),
            "app-rabbiz-unknown-4.scope"
        );
        assert_eq!(
            LifecycleManager::sanitize_scope_unit("   ...   ", 5),
            "app-rabbiz-unknown-5.scope"
        );
        assert_eq!(
            LifecycleManager::sanitize_scope_unit("日本語アプリ🎉", 6),
            "app-rabbiz-unknown-6.scope"
        );
        assert_eq!(
            LifecycleManager::sanitize_scope_unit("app/日本語/v2", 7),
            "app-rabbiz-app-v2-7.scope"
        );

        let long_id = "a".repeat(200);
        let unit = LifecycleManager::sanitize_scope_unit(&long_id, 99);
        assert_eq!(unit, format!("app-rabbiz-{}-99.scope", "a".repeat(64)));
        assert!(unit.len() < 128);
        assert!(unit.starts_with("app-rabbiz-"));
        assert!(unit.ends_with("-99.scope"));
    }

    #[test]
    fn relaunching_dead_app_allocates_new_instance_id() {
        let mut mgr = LifecycleManager::default();
        mgr.on_launch("org.kde.kalk", 100);
        mgr.on_mapped("org.kde.kalk", 200);
        mgr.on_exited("org.kde.kalk");
        assert_eq!(mgr.state_of("org.kde.kalk"), Some(AppState::Dead));

        let actions = mgr.on_launch("org.kde.kalk", 300);
        assert_eq!(mgr.state_of("org.kde.kalk"), Some(AppState::Starting));
        assert_eq!(
            actions,
            vec![LifecycleAction::ApplyPolicy {
                app_id: "org.kde.kalk".into(),
                scope: "app-rabbiz-org-kde-kalk-2.scope".into(),
                policy: CgroupPolicy::for_state(AppState::Starting),
            }]
        );
    }
}
