use serde::Deserialize;
use std::collections::BTreeMap;

use de_system::env::DESKTOP_CONTRACT_ENV;

use crate::desktop::DesktopEntry;

const EMBEDDED_LAUNCH_TOML: &str = include_str!("../data/launch.toml");

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct ContractConfig {
    env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LaunchOverride {
    pub exec: Option<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

/// Launch policy parsed from `data/launch.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LaunchPolicy {
    contract: ContractConfig,
    overrides: BTreeMap<String, LaunchOverride>,
}

/// Resolved command + environment ready to spawn via `Command`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

impl LaunchPolicy {
    /// Loads the compile-time embedded `data/launch.toml`.
    pub fn embedded() -> Result<Self, toml::de::Error> {
        Self::from_toml(EMBEDDED_LAUNCH_TOML)
    }

    /// Parses a `launch.toml` string.
    pub fn from_toml(raw: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(raw)
    }

    /// Resolves the final program, arguments, and environment for `entry`.
    /// The env base is the desktop contract from `de-system` (single
    /// authority), extended by `[contract.env]` then the app's override.
    pub fn resolve(&self, entry: &DesktopEntry) -> LaunchSpec {
        let mut env: BTreeMap<String, String> = DESKTOP_CONTRACT_ENV
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        for (k, v) in &self.contract.env {
            env.insert(k.clone(), v.clone());
        }

        if let Some(ov) = self.overrides.get(&entry.id) {
            for (k, v) in &ov.env {
                env.insert(k.clone(), v.clone());
            }
            if let Some(exec) = &ov.exec {
                return LaunchSpec {
                    program: exec.clone(),
                    args: ov.args.clone(),
                    env,
                };
            }
            let (program, mut args) = split_exec(&entry.exec);
            args.extend(ov.args.iter().cloned());
            return LaunchSpec { program, args, env };
        }

        let (program, args) = split_exec(&entry.exec);
        LaunchSpec { program, args, env }
    }
}

/// Splits an XDG `Exec=` string into `(program, args)`, stripping `%` field codes.
fn split_exec(exec: &str) -> (String, Vec<String>) {
    let mut parts = exec
        .split_whitespace()
        .filter(|tok| !is_field_code(tok))
        .map(str::to_string);
    let program = parts.next().unwrap_or_default();
    let args = parts.collect();
    (program, args)
}

fn is_field_code(tok: &str) -> bool {
    matches!(
        tok,
        "%f" | "%F" | "%u" | "%U" | "%d" | "%D" | "%n" | "%N" | "%i" | "%c" | "%k" | "%v" | "%m"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(id: &str, exec: &str) -> DesktopEntry {
        DesktopEntry {
            id: id.to_string(),
            name: id.to_string(),
            exec: exec.to_string(),
            icon: id.to_string(),
            categories: Vec::new(),
            no_display: false,
            hidden: false,
        }
    }

    #[test]
    fn embedded_launch_toml_overrides_chrome_and_sets_software_contract() {
        let policy = LaunchPolicy::embedded().expect("parse embedded launch.toml");
        let chrome = sample_entry("google-chrome", "/usr/bin/google-chrome-stable %U");
        let spec = policy.resolve(&chrome);

        assert_eq!(spec.program, "a21s-browser");
        assert_eq!(spec.args, vec!["--disable-gpu"]);
        assert_eq!(
            spec.env.get("A21S_FORCE_SOFTWARE").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            spec.env.get("QT_QUICK_BACKEND").map(String::as_str),
            Some("software")
        );
    }

    #[test]
    fn non_overridden_app_strips_field_codes_and_inherits_contract_env() {
        let policy = LaunchPolicy::embedded().expect("parse embedded launch.toml");
        let dolphin = sample_entry("org.kde.dolphin", "dolphin --new-window %u");
        let spec = policy.resolve(&dolphin);

        assert_eq!(spec.program, "dolphin");
        assert_eq!(spec.args, vec!["--new-window"]);
        assert_eq!(
            spec.env.get("A21S_FORCE_SOFTWARE").map(String::as_str),
            Some("1")
        );
    }
}
