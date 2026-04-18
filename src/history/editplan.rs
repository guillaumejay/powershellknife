use std::collections::HashMap;

use super::parse::Entry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Keep,
    Delete,
    Replace(String),
}

#[derive(Debug, Clone, Default)]
pub struct EditPlan {
    actions: HashMap<usize, Action>,
}

impl EditPlan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, entry_index: usize, action: Action) {
        match action {
            Action::Keep => {
                self.actions.remove(&entry_index);
            }
            a => {
                self.actions.insert(entry_index, a);
            }
        }
    }

    pub fn get(&self, entry_index: usize) -> &Action {
        self.actions.get(&entry_index).unwrap_or(&Action::Keep)
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn pending_count(&self) -> usize {
        self.actions.len()
    }

    pub fn render(&self, entries: &[Entry]) -> String {
        let mut out = String::new();
        for (idx, entry) in entries.iter().enumerate() {
            match self.get(idx) {
                Action::Delete => continue,
                Action::Keep => {
                    for line in &entry.raw_lines {
                        out.push_str(line);
                        out.push('\n');
                    }
                }
                Action::Replace(new_token) => {
                    let mut iter = entry.raw_lines.iter();
                    if let Some(first) = iter.next() {
                        out.push_str(&replace_first_token(first, new_token));
                        out.push('\n');
                    }
                    for line in iter {
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            }
        }
        out
    }

    pub fn preview(&self, entries: &[Entry]) -> String {
        let mut out = String::new();
        for (idx, entry) in entries.iter().enumerate() {
            match self.get(idx) {
                Action::Keep => continue,
                Action::Delete => {
                    for line in &entry.raw_lines {
                        out.push_str(&format!("- L{:<5} {}\n", entry.start_line, line));
                    }
                }
                Action::Replace(new_token) => {
                    if let Some(first) = entry.raw_lines.first() {
                        out.push_str(&format!("- L{:<5} {}\n", entry.start_line, first));
                        out.push_str(&format!(
                            "+ L{:<5} {}\n",
                            entry.start_line,
                            replace_first_token(first, new_token)
                        ));
                    }
                    for line in entry.raw_lines.iter().skip(1) {
                        out.push_str(&format!("  L{:<5} {}\n", entry.start_line, line));
                    }
                }
            }
        }
        out
    }
}

fn replace_first_token(line: &str, new_token: &str) -> String {
    let leading: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    let rest = &line[leading.len()..];
    let token_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let after = &rest[token_end..];
    format!("{leading}{new_token}{after}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::parse::parse;

    #[test]
    fn default_keeps_everything() {
        let entries = parse("Get-Process\nls\n");
        let plan = EditPlan::new();
        assert_eq!(plan.render(&entries), "Get-Process\nls\n");
        assert!(plan.is_empty());
    }

    #[test]
    fn delete_removes_entry() {
        let entries = parse("Get-Process\nls\nWrite-Host\n");
        let mut plan = EditPlan::new();
        plan.set(1, Action::Delete);
        assert_eq!(plan.render(&entries), "Get-Process\nWrite-Host\n");
    }

    #[test]
    fn replace_substitutes_first_token() {
        let entries = parse("Get-Procss -Name chrome\n");
        let mut plan = EditPlan::new();
        plan.set(0, Action::Replace("Get-Process".to_string()));
        assert_eq!(plan.render(&entries), "Get-Process -Name chrome\n");
    }

    #[test]
    fn replace_preserves_continuation_lines() {
        let entries = parse("Get-Procss | `\n  Sort-Object\n");
        let mut plan = EditPlan::new();
        plan.set(0, Action::Replace("Get-Process".to_string()));
        assert_eq!(plan.render(&entries), "Get-Process | `\n  Sort-Object\n");
    }

    #[test]
    fn replace_preserves_leading_whitespace() {
        let entries = parse("  Get-Procss -X\n");
        let mut plan = EditPlan::new();
        plan.set(0, Action::Replace("Get-Process".to_string()));
        assert_eq!(plan.render(&entries), "  Get-Process -X\n");
    }

    #[test]
    fn set_keep_clears_previous_action() {
        let entries = parse("ls\n");
        let mut plan = EditPlan::new();
        plan.set(0, Action::Delete);
        assert_eq!(plan.pending_count(), 1);
        plan.set(0, Action::Keep);
        assert_eq!(plan.pending_count(), 0);
        assert_eq!(plan.render(&entries), "ls\n");
    }

    #[test]
    fn preview_shows_deletions_and_replacements_only() {
        let entries = parse("Get-Process\nGet-Procss\nls\n");
        let mut plan = EditPlan::new();
        plan.set(1, Action::Replace("Get-Process".to_string()));
        plan.set(2, Action::Delete);
        let preview = plan.preview(&entries);
        assert!(preview.contains("- L2"));
        assert!(preview.contains("+ L2"));
        assert!(preview.contains("Get-Procss"));
        assert!(preview.contains("Get-Process"));
        assert!(preview.contains("- L3"));
        assert!(!preview.contains("L1"));
    }
}
