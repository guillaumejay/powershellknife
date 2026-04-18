use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command as ProcessCommand;

use crate::backup;

pub const STALE_AFTER_DAYS: i64 = 30;

const CORE_CMDLETS_JSON: &str = include_str!("core_cmdlets.json");

const PWSH_SCAN_COMMAND: &str = "Get-Command | \
    Select-Object @{n='name';e={$_.Name}},@{n='kind';e={$_.CommandType.ToString()}} | \
    ConvertTo-Json -Compress -AsArray";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Embedded,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    pub generated_at: DateTime<Utc>,
    pub source: Source,
    pub commands: Vec<Command>,
}

impl Inventory {
    pub fn embedded() -> Result<Self> {
        let commands: Vec<Command> =
            serde_json::from_str(CORE_CMDLETS_JSON).context("parsing embedded core cmdlets")?;
        Ok(Self {
            generated_at: Utc::now(),
            source: Source::Embedded,
            commands,
        })
    }

    pub fn contains_name(&self, name: &str) -> bool {
        self.commands
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn is_stale(&self) -> bool {
        Utc::now() - self.generated_at > Duration::days(STALE_AFTER_DAYS)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("serializing inventory")?;
        backup::atomic_write_str(path, &json)
    }

    pub fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("reading inventory {}", path.display()))?;
        let inv: Self = serde_json::from_str(&content)
            .with_context(|| format!("parsing inventory {}", path.display()))?;
        Ok(Some(inv))
    }

    pub fn load_or_embedded(path: &Path) -> Result<Self> {
        match Self::load(path)? {
            Some(inv) => Ok(inv),
            None => Self::embedded(),
        }
    }
}

pub fn scan_system() -> Result<Inventory> {
    let output = ProcessCommand::new("pwsh")
        .args(["-NoProfile", "-NoLogo", "-Command", PWSH_SCAN_COMMAND])
        .output()
        .context("invoking pwsh to scan commands (is PowerShell 7 installed?)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("pwsh scan failed: {}", stderr.trim());
    }

    let json = String::from_utf8(output.stdout).context("pwsh output is not valid UTF-8")?;
    let commands: Vec<Command> =
        serde_json::from_str(json.trim()).context("parsing pwsh Get-Command output")?;

    Ok(Inventory {
        generated_at: Utc::now(),
        source: Source::System,
        commands,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn embedded_loads_and_has_known_cmdlets() {
        let inv = Inventory::embedded().expect("embedded");
        assert!(inv.contains_name("Get-Command"));
        assert!(inv.contains_name("Get-Process"));
        assert!(inv.contains_name("ls"));
        assert!(inv.commands.len() > 100);
    }

    #[test]
    fn contains_name_is_case_insensitive() {
        let inv = Inventory::embedded().expect("embedded");
        assert!(inv.contains_name("get-command"));
        assert!(inv.contains_name("GET-COMMAND"));
        assert!(!inv.contains_name("Get-NotReal"));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("inv.json");
        let inv = Inventory::embedded().unwrap();
        inv.save(&path).unwrap();
        let loaded = Inventory::load(&path).unwrap().expect("some");
        assert_eq!(loaded.commands.len(), inv.commands.len());
        assert!(loaded.contains_name("Get-Command"));
    }

    #[test]
    fn load_returns_none_when_missing() {
        let td = TempDir::new().unwrap();
        let res = Inventory::load(&td.path().join("nope.json")).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn load_or_embedded_falls_back_when_missing() {
        let td = TempDir::new().unwrap();
        let inv = Inventory::load_or_embedded(&td.path().join("nope.json")).unwrap();
        assert_eq!(inv.source, Source::Embedded);
    }

    #[test]
    fn is_stale_true_for_old_timestamp() {
        let mut inv = Inventory::embedded().unwrap();
        inv.generated_at = Utc::now() - Duration::days(STALE_AFTER_DAYS + 1);
        assert!(inv.is_stale());
    }

    #[test]
    fn is_stale_false_for_fresh_timestamp() {
        let inv = Inventory::embedded().unwrap();
        assert!(!inv.is_stale());
    }

    #[test]
    #[ignore = "requires pwsh on PATH; run with `cargo test -- --ignored`"]
    fn scan_system_returns_commands() {
        let inv = scan_system().expect("scan");
        assert_eq!(inv.source, Source::System);
        assert!(inv.commands.len() > 100);
        assert!(inv.contains_name("Get-Command"));
    }
}
