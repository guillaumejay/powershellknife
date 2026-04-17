use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn history_file() -> Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").context("APPDATA environment variable is not set")?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("PowerShell")
        .join("PSReadLine")
        .join("ConsoleHost_history.txt"))
}

pub fn profile_file() -> Result<PathBuf> {
    let documents =
        dirs::document_dir().context("Could not determine the user's Documents directory")?;
    Ok(documents
        .join("PowerShell")
        .join("Microsoft.PowerShell_profile.ps1"))
}

pub fn app_data_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine the user's home directory")?;
    Ok(home.join(".powershellknife"))
}

pub fn backups_dir() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("backups"))
}

pub fn inventory_cache() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("cmdlet_inventory.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_file_ends_with_known_filename() {
        if std::env::var_os("APPDATA").is_some() {
            let p = history_file().expect("history_file");
            assert!(p.ends_with("ConsoleHost_history.txt"));
        }
    }

    #[test]
    fn profile_file_ends_with_known_filename() {
        if dirs::document_dir().is_some() {
            let p = profile_file().expect("profile_file");
            assert!(p.ends_with("Microsoft.PowerShell_profile.ps1"));
        }
    }

    #[test]
    fn backups_dir_is_under_app_data_dir() {
        if dirs::home_dir().is_some() {
            let app = app_data_dir().expect("app_data_dir");
            let bk = backups_dir().expect("backups_dir");
            assert!(bk.starts_with(&app));
        }
    }
}
