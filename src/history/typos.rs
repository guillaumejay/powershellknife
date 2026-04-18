use crate::inventory::Inventory;

use super::parse::Entry;

pub const DEFAULT_DENYLIST: &[&str] = &[
    "git",
    "hg",
    "svn",
    "docker",
    "podman",
    "kubectl",
    "helm",
    "npm",
    "yarn",
    "pnpm",
    "pip",
    "pip3",
    "cargo",
    "rustup",
    "rustc",
    "bundle",
    "gem",
    "composer",
    "node",
    "deno",
    "bun",
    "python",
    "python3",
    "ruby",
    "java",
    "javac",
    "dotnet",
    "go",
    "aws",
    "gcloud",
    "code",
    "subl",
    "vim",
    "nvim",
    "emacs",
    "nano",
    "ssh",
    "scp",
    "sftp",
    "rsync",
    "make",
    "cmake",
    "ninja",
    "meson",
    "gcc",
    "g++",
    "clang",
    "clang++",
    "openssl",
    "gpg",
    "tar",
    "zip",
    "unzip",
    "7z",
    "ffmpeg",
    "imagemagick",
    "terraform",
    "tofu",
    "vault",
    "psql",
    "mysql",
    "sqlite3",
    "mongosh",
    "redis-cli",
    "jq",
    "yq",
    "wget",
];

const MAX_DISTANCE: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypoFlag {
    pub entry_index: usize,
    pub first_token: String,
    pub suggestion: Option<String>,
}

pub fn find_typos(entries: &[Entry], inventory: &Inventory, denylist: &[&str]) -> Vec<TypoFlag> {
    let mut flags = Vec::new();
    for (idx, entry) in entries.iter().enumerate() {
        let Some(token) = first_token(&entry.command) else {
            continue;
        };
        if is_path_like(token) {
            continue;
        }
        if is_denylisted(token, denylist) {
            continue;
        }
        if inventory.contains_name(token) {
            continue;
        }
        flags.push(TypoFlag {
            entry_index: idx,
            first_token: token.to_string(),
            suggestion: best_suggestion(token, inventory),
        });
    }
    flags
}

pub fn high_confidence(flag: &TypoFlag) -> bool {
    flag.suggestion.is_some()
}

fn first_token(command: &str) -> Option<&str> {
    command.split_whitespace().next()
}

fn is_path_like(token: &str) -> bool {
    token.starts_with(".\\")
        || token.starts_with("./")
        || token.starts_with('/')
        || token.starts_with('~')
        || has_drive_prefix(token)
}

fn has_drive_prefix(s: &str) -> bool {
    let mut chars = s.chars();
    let first = chars.next();
    let second = chars.next();
    let third = chars.next();
    matches!(first, Some(c) if c.is_ascii_alphabetic())
        && matches!(second, Some(':'))
        && matches!(third, Some('\\' | '/'))
}

fn is_denylisted(token: &str, denylist: &[&str]) -> bool {
    denylist.iter().any(|d| d.eq_ignore_ascii_case(token))
}

fn best_suggestion(token: &str, inventory: &Inventory) -> Option<String> {
    let token_lower = token.to_ascii_lowercase();
    let first_char = token_lower.chars().next()?;

    let mut candidates: Vec<(String, usize)> = inventory
        .commands
        .iter()
        .filter_map(|c| {
            let name_lower = c.name.to_ascii_lowercase();
            if name_lower.chars().next()? != first_char {
                return None;
            }
            let d = strsim::levenshtein(&token_lower, &name_lower);
            if d == 0 || d > MAX_DISTANCE {
                return None;
            }
            Some((c.name.clone(), d))
        })
        .collect();

    candidates.sort_by_key(|(_, d)| *d);

    match candidates.as_slice() {
        [] => None,
        [(name, _)] => Some(name.clone()),
        [(name, d1), (_, d2), ..] if d2 > d1 => Some(name.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::parse::parse;

    fn inv() -> Inventory {
        Inventory::embedded().unwrap()
    }

    #[test]
    fn close_typo_gets_suggestion() {
        let entries = parse("Get-Procss -Name chrome\n");
        let flags = find_typos(&entries, &inv(), DEFAULT_DENYLIST);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].first_token, "Get-Procss");
        assert_eq!(flags[0].suggestion.as_deref(), Some("Get-Process"));
    }

    #[test]
    fn valid_cmdlet_is_not_flagged() {
        let entries = parse("Get-Process\nls\nGet-ChildItem\n");
        let flags = find_typos(&entries, &inv(), DEFAULT_DENYLIST);
        assert!(flags.is_empty());
    }

    #[test]
    fn external_tool_is_skipped() {
        let entries = parse("git status\ndocker ps\nnpm install\ncargo build\n");
        let flags = find_typos(&entries, &inv(), DEFAULT_DENYLIST);
        assert!(flags.is_empty());
    }

    #[test]
    fn path_like_tokens_are_skipped() {
        let entries =
            parse(".\\script.ps1\n./deploy.sh\nC:\\tools\\foo.exe\n/usr/bin/env\n~/bin/thing\n");
        let flags = find_typos(&entries, &inv(), DEFAULT_DENYLIST);
        assert!(flags.is_empty());
    }

    #[test]
    fn unknown_without_close_match_is_flagged_without_suggestion() {
        let entries = parse("docer ps\n");
        let flags = find_typos(&entries, &inv(), DEFAULT_DENYLIST);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].first_token, "docer");
        assert!(flags[0].suggestion.is_none());
    }

    #[test]
    fn case_insensitive_inventory_hit() {
        let entries = parse("get-process\n");
        let flags = find_typos(&entries, &inv(), DEFAULT_DENYLIST);
        assert!(flags.is_empty());
    }

    #[test]
    fn candidate_restricted_to_same_first_letter() {
        let entries = parse("xet-process\n");
        let flags = find_typos(&entries, &inv(), DEFAULT_DENYLIST);
        assert_eq!(flags.len(), 1);
        assert!(flags[0].suggestion.is_none());
    }

    #[test]
    fn high_confidence_helper_matches_suggestion_presence() {
        let with = TypoFlag {
            entry_index: 0,
            first_token: "x".into(),
            suggestion: Some("y".into()),
        };
        let without = TypoFlag {
            entry_index: 0,
            first_token: "x".into(),
            suggestion: None,
        };
        assert!(high_confidence(&with));
        assert!(!high_confidence(&without));
    }
}
