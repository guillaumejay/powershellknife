#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub command: String,
    pub raw_lines: Vec<String>,
    pub start_line: usize,
}

pub fn parse(content: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut current_raw: Vec<String> = Vec::new();
    let mut current_parts: Vec<String> = Vec::new();
    let mut start_line: usize = 0;

    for (idx, line) in content.lines().enumerate() {
        if current_raw.is_empty() && line.trim().is_empty() {
            continue;
        }
        if current_raw.is_empty() {
            start_line = idx + 1;
        }
        current_raw.push(line.to_string());

        let trimmed_end = line.trim_end();
        if let Some(stripped) = trimmed_end.strip_suffix('`') {
            current_parts.push(stripped.to_string());
        } else {
            current_parts.push(line.to_string());
            entries.push(Entry {
                command: current_parts.join(" ").trim().to_string(),
                raw_lines: std::mem::take(&mut current_raw),
                start_line,
            });
            current_parts.clear();
        }
    }

    if !current_raw.is_empty() {
        entries.push(Entry {
            command: current_parts.join(" ").trim().to_string(),
            raw_lines: current_raw,
            start_line,
        });
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_entries() {
        let content = "Get-Process\nGet-ChildItem\n";
        let entries = parse(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].command, "Get-Process");
        assert_eq!(entries[0].start_line, 1);
        assert_eq!(entries[1].command, "Get-ChildItem");
        assert_eq!(entries[1].start_line, 2);
    }

    #[test]
    fn backtick_continuation_joins_lines() {
        let content = "Get-ChildItem | `\n  Where-Object { $_.Name }\n";
        let entries = parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].command,
            "Get-ChildItem |    Where-Object { $_.Name }"
        );
        assert_eq!(entries[0].raw_lines.len(), 2);
        assert_eq!(entries[0].start_line, 1);
    }

    #[test]
    fn multiple_entries_with_continuation() {
        let content = "Get-Process\nGet-ChildItem | `\n  Sort-Object\nWrite-Host hi\n";
        let entries = parse(content);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].start_line, 1);
        assert_eq!(entries[1].start_line, 2);
        assert_eq!(entries[1].raw_lines.len(), 2);
        assert_eq!(entries[2].start_line, 4);
    }

    #[test]
    fn trailing_continuation_without_closing_line() {
        let content = "Get-Process `\n";
        let entries = parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].raw_lines, vec!["Get-Process `"]);
    }

    #[test]
    fn blank_lines_between_entries_are_dropped() {
        let content = "Get-Process\n\nGet-ChildItem\n";
        let entries = parse(content);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(parse("").is_empty());
    }

    #[test]
    fn no_trailing_newline() {
        let content = "Get-Process";
        let entries = parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].command, "Get-Process");
    }
}
