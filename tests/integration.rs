//! End-to-end tests exercising the library-level flows against real
//! temporary files. These are the safety nets for the pieces that stitch
//! together parsing, planning, backups, and atomic writes.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use powershellknife::backup;
use powershellknife::history::{
    dedup::find_duplicates,
    editplan::{Action, EditPlan},
    parse::parse,
    typos::{DEFAULT_DENYLIST, find_typos},
};
use powershellknife::inventory::Inventory;
use powershellknife::profile::{block, settings::Settings};

struct TestEnv {
    _td: TempDir,
    root: PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let td = TempDir::new().expect("tempdir");
        let root = td.path().to_path_buf();
        Self { _td: td, root }
    }

    fn write(&self, name: &str, content: &str) -> PathBuf {
        let p = self.root.join(name);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, content).unwrap();
        p
    }
}

#[test]
fn history_end_to_end_cleans_typos_and_dups() {
    let env = TestEnv::new();
    let history = env.write(
        "ConsoleHost_history.txt",
        "Get-Process\nGet-Procss chrome\nls\nls\nls\ndocer ps\n",
    );
    let backups_root = env.root.join("backups");

    let content = fs::read_to_string(&history).unwrap();
    let entries = parse(&content);
    let dups = find_duplicates(&entries);
    let inv = Inventory::embedded().unwrap();
    let typos = find_typos(&entries, &inv, DEFAULT_DENYLIST);

    // Classic user workflow: auto-fix the one high-confidence typo,
    // collapse the `ls` group, and leave the unrecognized `docer` alone.
    let mut plan = EditPlan::new();
    for flag in &typos {
        if let Some(sugg) = flag.suggestion.clone() {
            plan.set(flag.entry_index, Action::Replace(sugg));
        }
    }
    for group in &dups {
        let latest = *group.entry_indices.last().unwrap();
        for &idx in &group.entry_indices {
            if idx != latest {
                plan.set(idx, Action::Delete);
            }
        }
    }

    let backup_created = backup::create(&backups_root, &[history.as_path()]).unwrap();
    assert!(
        backup_created.dir.join("ConsoleHost_history.txt").exists(),
        "backup should mirror the original file"
    );

    let new_body = plan.render(&entries);
    backup::atomic_write_str(&history, &new_body).unwrap();

    let final_content = fs::read_to_string(&history).unwrap();
    let final_lines: Vec<&str> = final_content.lines().collect();
    assert_eq!(
        final_lines,
        vec!["Get-Process", "Get-Process chrome", "ls", "docer ps"],
        "expected typo replaced, dups collapsed to the last occurrence, unknown preserved"
    );

    // Backup still holds the pristine copy.
    let backup_copy =
        fs::read_to_string(backup_created.dir.join("ConsoleHost_history.txt")).unwrap();
    assert!(backup_copy.contains("Get-Procss"));
    assert!(backup_copy.matches("ls\n").count() >= 3);
}

#[test]
fn profile_end_to_end_preserves_custom_code() {
    let env = TestEnv::new();
    let original = format!(
        "# my header\n\
         function prompt {{ 'PS> ' }}\n\
         \n\
         {}\n\
         Set-PSReadLineOption -HistoryNoDuplicates\n\
         Import-Module posh-git\n\
         {}\n\
         \n\
         # trailing user code\n\
         $LASTEXITCODE = 0\n",
        block::START_MARKER,
        block::END_MARKER,
    );
    let profile = env.write("profile.ps1", &original);

    let content = fs::read_to_string(&profile).unwrap();
    let state = block::locate(&content);
    let slice = match state {
        block::BlockState::Present(s) => s,
        other => panic!("expected present block, got {other:?}"),
    };
    let mut settings = Settings::parse(&slice.inner_lines);

    // Simulate: user toggles PredictionSource to History and adds an alias.
    settings.psreadline.prediction_source =
        powershellknife::profile::settings::PredictionSource::History;
    settings
        .aliases
        .insert("ll".to_string(), "Get-ChildItem -Force".to_string());

    let rewritten = block::compose(&content, &settings.serialize()).unwrap();
    let backups_root = env.root.join("backups");
    backup::create(&backups_root, &[profile.as_path()]).unwrap();
    backup::atomic_write_str(&profile, &rewritten).unwrap();

    let final_content = fs::read_to_string(&profile).unwrap();

    // Custom code outside the block is preserved byte-exact (prefix + suffix).
    assert!(final_content.starts_with("# my header\nfunction prompt { 'PS> ' }\n\n"));
    assert!(final_content.ends_with("\n\n# trailing user code\n$LASTEXITCODE = 0\n"));

    // Block contents reflect the new model deterministically.
    assert!(final_content.contains("Set-PSReadLineOption -HistoryNoDuplicates:$true"));
    assert!(final_content.contains("Set-PSReadLineOption -PredictionSource History"));
    assert!(final_content.contains("Import-Module posh-git"));
    assert!(final_content.contains("Set-Alias ll 'Get-ChildItem -Force'"));

    // Re-parse: model round-trips without drift.
    let state2 = block::locate(&final_content);
    let slice2 = match state2 {
        block::BlockState::Present(s) => s,
        other => panic!("expected present block after write, got {other:?}"),
    };
    let reparsed = Settings::parse(&slice2.inner_lines);
    assert_eq!(reparsed, settings);
}

#[test]
fn profile_end_to_end_appends_block_when_missing() {
    let env = TestEnv::new();
    let original = "# pristine user profile\n$x = 42\n";
    let profile = env.write("profile.ps1", original);

    let content = fs::read_to_string(&profile).unwrap();
    assert!(matches!(
        block::locate(&content),
        block::BlockState::Missing
    ));

    let mut settings = Settings::default();
    settings.psreadline.history_no_duplicates = true;
    settings.modules.push("Terminal-Icons".to_string());

    let rewritten = block::compose(&content, &settings.serialize()).unwrap();
    backup::atomic_write_str(&profile, &rewritten).unwrap();

    let final_content = fs::read_to_string(&profile).unwrap();
    assert!(final_content.starts_with("# pristine user profile\n$x = 42\n"));
    assert!(final_content.contains(block::START_MARKER));
    assert!(final_content.contains("Import-Module Terminal-Icons"));
    assert!(final_content.contains(block::END_MARKER));
}

#[test]
fn profile_refuses_write_when_block_corrupted() {
    let env = TestEnv::new();
    let corrupted = format!(
        "pre\n{}\n{}\nfoo\n{}\n",
        block::START_MARKER,
        block::START_MARKER,
        block::END_MARKER
    );
    let profile = env.write("profile.ps1", &corrupted);

    let content = fs::read_to_string(&profile).unwrap();
    assert!(matches!(
        block::locate(&content),
        block::BlockState::Corrupted(_)
    ));
    let err = block::compose(&content, &[]).unwrap_err();
    assert!(format!("{err}").contains("corrupted"));

    // File on disk is untouched.
    assert_eq!(fs::read_to_string(&profile).unwrap(), corrupted);
}

#[test]
fn backup_roundtrip_restores_exact_bytes() {
    let env = TestEnv::new();
    let target = env.write("data.txt", "original content\nline two\n");
    let backups_root = env.root.join("backups");

    let backup_info = backup::create(&backups_root, &[target.as_path()]).unwrap();
    backup::atomic_write_str(&target, "tampered!\n").unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "tampered!\n");

    // Manually restore from the backup dir (mirrors what `psknife restore` does,
    // without touching the user's real filesystem).
    let saved = backup_info.dir.join("data.txt");
    let restored_bytes = fs::read(&saved).unwrap();
    backup::atomic_write_bytes(&target, &restored_bytes).unwrap();

    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "original content\nline two\n"
    );
}

#[test]
fn atomic_write_never_leaves_temp_file_on_success() {
    let env = TestEnv::new();
    let target = env.root.join("nested").join("file.txt");
    backup::atomic_write_str(&target, "hello").unwrap();

    let parent_entries: Vec<_> = fs::read_dir(target.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(parent_entries.len(), 1);
    assert_eq!(parent_entries[0], "file.txt");
}
