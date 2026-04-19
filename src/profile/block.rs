use anyhow::{Result, bail};

pub const START_MARKER: &str = "# >>> managed by powershellknife — do not edit manually";
pub const END_MARKER: &str = "# <<< managed by powershellknife";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockState {
    Missing,
    Present(BlockSlice),
    Corrupted(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSlice {
    pub prefix: String,
    pub inner_lines: Vec<String>,
    pub suffix: String,
}

pub fn locate(content: &str) -> BlockState {
    let segments: Vec<&str> = content.split_inclusive('\n').collect();

    let starts: Vec<usize> = segments
        .iter()
        .enumerate()
        .filter(|(_, s)| trim_eol(s) == START_MARKER)
        .map(|(i, _)| i)
        .collect();
    let ends: Vec<usize> = segments
        .iter()
        .enumerate()
        .filter(|(_, s)| trim_eol(s) == END_MARKER)
        .map(|(i, _)| i)
        .collect();

    match (starts.as_slice(), ends.as_slice()) {
        ([], []) => BlockState::Missing,
        ([s], [e]) if e > s => {
            let prefix: String = segments[..*s].concat();
            let suffix: String = segments[e + 1..].concat();
            let inner_lines: Vec<String> = segments[s + 1..*e]
                .iter()
                .map(|s| trim_eol(s).to_string())
                .collect();
            BlockState::Present(BlockSlice {
                prefix,
                inner_lines,
                suffix,
            })
        }
        ([_s], [_e]) => BlockState::Corrupted(
            "managed block markers are out of order (end before start)".to_string(),
        ),
        (ss, es) => {
            let mut msg = String::from("managed block markers are corrupted");
            if ss.len() > 1 {
                msg.push_str(&format!(" (found {} start markers)", ss.len()));
            }
            if es.len() > 1 {
                msg.push_str(&format!(" (found {} end markers)", es.len()));
            }
            if ss.is_empty() && !es.is_empty() {
                msg.push_str(" (end marker without start)");
            }
            if es.is_empty() && !ss.is_empty() {
                msg.push_str(" (start marker without end)");
            }
            BlockState::Corrupted(msg)
        }
    }
}

pub fn compose(original: &str, new_inner_lines: &[String]) -> Result<String> {
    match locate(original) {
        BlockState::Corrupted(msg) => bail!("{msg}"),
        BlockState::Present(slice) => {
            let mut out = String::with_capacity(original.len() + 64);
            out.push_str(&slice.prefix);
            push_block(&mut out, new_inner_lines);
            out.push_str(&slice.suffix);
            Ok(out)
        }
        BlockState::Missing => {
            let mut out = String::with_capacity(original.len() + 256);
            out.push_str(original);
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            if !out.is_empty() {
                out.push('\n');
            }
            push_block(&mut out, new_inner_lines);
            Ok(out)
        }
    }
}

fn trim_eol(s: &str) -> &str {
    s.trim_end_matches(['\r', '\n'])
}

fn push_block(out: &mut String, inner_lines: &[String]) {
    out.push_str(START_MARKER);
    out.push('\n');
    for line in inner_lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(END_MARKER);
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_missing_when_no_markers() {
        let content = "Write-Host 'hi'\nImport-Module Foo\n";
        assert_eq!(locate(content), BlockState::Missing);
    }

    #[test]
    fn locate_present_extracts_inner_lines() {
        let content = format!(
            "before\n{START_MARKER}\nSet-PSReadLineOption -HistoryNoDuplicates\nImport-Module posh-git\n{END_MARKER}\nafter\n"
        );
        match locate(&content) {
            BlockState::Present(slice) => {
                assert_eq!(slice.prefix, "before\n");
                assert_eq!(slice.suffix, "after\n");
                assert_eq!(
                    slice.inner_lines,
                    vec![
                        "Set-PSReadLineOption -HistoryNoDuplicates".to_string(),
                        "Import-Module posh-git".to_string(),
                    ]
                );
            }
            other => panic!("expected Present, got {other:?}"),
        }
    }

    #[test]
    fn locate_corrupted_with_duplicate_start() {
        let content = format!("{START_MARKER}\n{START_MARKER}\nx\n{END_MARKER}\n");
        assert!(matches!(locate(&content), BlockState::Corrupted(_)));
    }

    #[test]
    fn locate_corrupted_with_duplicate_end() {
        let content = format!("{START_MARKER}\nx\n{END_MARKER}\n{END_MARKER}\n");
        assert!(matches!(locate(&content), BlockState::Corrupted(_)));
    }

    #[test]
    fn locate_corrupted_when_end_before_start() {
        let content = format!("{END_MARKER}\nx\n{START_MARKER}\n");
        assert!(matches!(locate(&content), BlockState::Corrupted(_)));
    }

    #[test]
    fn locate_corrupted_when_only_end_marker() {
        let content = format!("x\n{END_MARKER}\ny\n");
        assert!(matches!(locate(&content), BlockState::Corrupted(_)));
    }

    #[test]
    fn locate_handles_crlf_line_endings() {
        let content = format!("before\r\n{START_MARKER}\r\nx\r\n{END_MARKER}\r\nafter\r\n");
        assert!(matches!(locate(&content), BlockState::Present(_)));
    }

    #[test]
    fn compose_replaces_existing_block() {
        let original =
            format!("prefix line\n{START_MARKER}\nold line\n{END_MARKER}\nsuffix line\n");
        let new_inner = vec!["a".to_string(), "b".to_string()];
        let out = compose(&original, &new_inner).unwrap();
        let expected = format!("prefix line\n{START_MARKER}\na\nb\n{END_MARKER}\nsuffix line\n");
        assert_eq!(out, expected);
    }

    #[test]
    fn compose_preserves_code_outside_block() {
        let original = format!(
            "# user header\nfunction My-Thing {{ 'hi' }}\n\n{START_MARKER}\nImport-Module Foo\n{END_MARKER}\n\n# trailing user code\n$x = 1\n"
        );
        let new_inner = vec!["Import-Module Bar".to_string()];
        let out = compose(&original, &new_inner).unwrap();
        assert!(out.starts_with("# user header\nfunction My-Thing { 'hi' }\n\n"));
        assert!(out.ends_with("\n\n# trailing user code\n$x = 1\n"));
        assert!(out.contains("Import-Module Bar"));
        assert!(!out.contains("Import-Module Foo"));
    }

    #[test]
    fn compose_appends_block_when_missing() {
        let original = "# just my stuff\n$x = 1\n";
        let new_inner = vec!["Import-Module Foo".to_string()];
        let out = compose(original, &new_inner).unwrap();
        assert!(out.starts_with("# just my stuff\n$x = 1\n"));
        assert!(out.contains(START_MARKER));
        assert!(out.contains("Import-Module Foo"));
        assert!(out.contains(END_MARKER));
        assert!(out.ends_with(&format!("{END_MARKER}\n")));
    }

    #[test]
    fn compose_appends_block_to_file_without_trailing_newline() {
        let original = "$x = 1";
        let out = compose(original, &[]).unwrap();
        assert!(out.starts_with("$x = 1\n"));
        assert!(out.contains(START_MARKER));
    }

    #[test]
    fn compose_writes_block_to_empty_file() {
        let out = compose("", &["Import-Module Foo".to_string()]).unwrap();
        assert_eq!(
            out,
            format!("{START_MARKER}\nImport-Module Foo\n{END_MARKER}\n")
        );
    }

    #[test]
    fn compose_refuses_corrupted_block() {
        let original = format!("{START_MARKER}\n{START_MARKER}\nx\n{END_MARKER}\n");
        let err = compose(&original, &[]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("corrupted"));
    }

    #[test]
    fn locate_then_compose_is_byte_exact_with_same_inner() {
        let original = format!("alpha\n{START_MARKER}\none\ntwo\n{END_MARKER}\nomega\n");
        let slice = match locate(&original) {
            BlockState::Present(s) => s,
            other => panic!("expected Present, got {other:?}"),
        };
        let out = compose(&original, &slice.inner_lines).unwrap();
        assert_eq!(out, original);
    }
}
