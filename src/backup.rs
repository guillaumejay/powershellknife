use anyhow::{Context, Result, bail};
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};

use crate::paths;

pub struct Backup {
    pub dir: PathBuf,
    pub timestamp: String,
}

pub fn create(root: &Path, files: &[&Path]) -> Result<Backup> {
    let timestamp = Local::now().format("%Y-%m-%d_%H%M%S").to_string();
    let dir = root.join(&timestamp);
    fs::create_dir_all(&dir).with_context(|| format!("creating backup dir {}", dir.display()))?;

    for file in files {
        if !file.exists() {
            continue;
        }
        let name = file
            .file_name()
            .with_context(|| format!("backup source has no file name: {}", file.display()))?;
        let dest = dir.join(name);
        fs::copy(file, &dest)
            .with_context(|| format!("copying {} to {}", file.display(), dest.display()))?;
    }

    Ok(Backup { dir, timestamp })
}

pub fn list(root: &Path) -> Result<Vec<Backup>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let timestamp = entry.file_name().to_string_lossy().into_owned();
        out.push(Backup {
            dir: entry.path(),
            timestamp,
        });
    }
    out.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(out)
}

pub fn restore(timestamp: Option<&str>) -> Result<()> {
    let root = paths::backups_dir()?;
    let backups = list(&root)?;
    if backups.is_empty() {
        bail!("no backups found under {}", root.display());
    }

    println!("available backups ({}):", backups.len());
    for b in &backups {
        println!("  {}", b.timestamp);
    }

    let backup = match timestamp {
        Some(t) => backups
            .into_iter()
            .find(|b| b.timestamp == t)
            .with_context(|| format!("no backup found with timestamp {t}"))?,
        None => backups.into_iter().next().expect("non-empty"),
    };

    let history = paths::history_file()?;
    let profile = paths::profile_file()?;
    let mut restored = 0;
    restored += restore_one(&backup, &history)?;
    restored += restore_one(&backup, &profile)?;

    println!(
        "\nrestored {} file(s) from backup {}",
        restored, backup.timestamp
    );
    Ok(())
}

fn restore_one(backup: &Backup, dest: &Path) -> Result<u32> {
    let name = dest
        .file_name()
        .with_context(|| format!("destination has no file name: {}", dest.display()))?;
    let src = backup.dir.join(name);
    if !src.exists() {
        return Ok(0);
    }
    let contents =
        fs::read(&src).with_context(|| format!("reading backup source {}", src.display()))?;
    atomic_write_bytes(dest, &contents)?;
    println!("  restored {}", dest.display());
    Ok(1)
}

pub fn atomic_write_bytes(dest: &Path, contents: &[u8]) -> Result<()> {
    let parent = dest
        .parent()
        .with_context(|| format!("destination has no parent: {}", dest.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("creating parent dir for {}", dest.display()))?;

    let mut tmp_name = dest
        .file_name()
        .with_context(|| format!("destination has no file name: {}", dest.display()))?
        .to_owned();
    tmp_name.push(".psknife-tmp");
    let tmp = dest.with_file_name(tmp_name);

    fs::write(&tmp, contents).with_context(|| format!("writing temp file {}", tmp.display()))?;
    fs::rename(&tmp, dest)
        .with_context(|| format!("renaming {} to {}", tmp.display(), dest.display()))?;
    Ok(())
}

pub fn atomic_write_str(dest: &Path, contents: &str) -> Result<()> {
    atomic_write_bytes(dest, contents.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn atomic_write_creates_destination() {
        let td = TempDir::new().unwrap();
        let dest = td.path().join("foo.txt");
        atomic_write_str(&dest, "hello").unwrap();
        assert_eq!(fs::read_to_string(&dest).unwrap(), "hello");
    }

    #[test]
    fn atomic_write_overwrites_existing() {
        let td = TempDir::new().unwrap();
        let dest = td.path().join("foo.txt");
        fs::write(&dest, "old").unwrap();
        atomic_write_str(&dest, "new").unwrap();
        assert_eq!(fs::read_to_string(&dest).unwrap(), "new");
    }

    #[test]
    fn atomic_write_leaves_no_tmp_file() {
        let td = TempDir::new().unwrap();
        let dest = td.path().join("foo.txt");
        atomic_write_str(&dest, "x").unwrap();
        let count = fs::read_dir(td.path()).unwrap().count();
        assert_eq!(count, 1);
    }

    #[test]
    fn atomic_write_creates_parent_dirs() {
        let td = TempDir::new().unwrap();
        let dest = td.path().join("nested").join("deep").join("foo.txt");
        atomic_write_str(&dest, "x").unwrap();
        assert!(dest.exists());
    }

    #[test]
    fn create_copies_existing_files_and_skips_missing() {
        let td = TempDir::new().unwrap();
        let root = td.path().join("backups");
        let src_ok = td.path().join("ok.txt");
        fs::write(&src_ok, "content").unwrap();
        let src_missing = td.path().join("missing.txt");

        let backup = create(&root, &[src_ok.as_path(), src_missing.as_path()]).unwrap();

        assert!(backup.dir.starts_with(&root));
        assert_eq!(
            fs::read_to_string(backup.dir.join("ok.txt")).unwrap(),
            "content"
        );
        assert!(!backup.dir.join("missing.txt").exists());
    }

    #[test]
    fn list_returns_newest_first() {
        let td = TempDir::new().unwrap();
        let root = td.path().join("backups");
        fs::create_dir_all(root.join("2026-01-01_120000")).unwrap();
        fs::create_dir_all(root.join("2026-02-01_120000")).unwrap();
        fs::create_dir_all(root.join("2025-12-31_235959")).unwrap();

        let backups = list(&root).unwrap();
        let timestamps: Vec<_> = backups.iter().map(|b| b.timestamp.as_str()).collect();
        assert_eq!(
            timestamps,
            [
                "2026-02-01_120000",
                "2026-01-01_120000",
                "2025-12-31_235959"
            ]
        );
    }

    #[test]
    fn list_returns_empty_when_root_missing() {
        let td = TempDir::new().unwrap();
        let backups = list(&td.path().join("nope")).unwrap();
        assert!(backups.is_empty());
    }
}
