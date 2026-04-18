use super::parse::Entry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub command: String,
    pub entry_indices: Vec<usize>,
}

pub fn find_duplicates(entries: &[Entry]) -> Vec<DuplicateGroup> {
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, entry) in entries.iter().enumerate() {
        let key = entry.command.trim().to_string();
        if key.is_empty() {
            continue;
        }
        groups.entry(key).or_default().push(idx);
    }

    let mut out: Vec<DuplicateGroup> = groups
        .into_iter()
        .filter(|(_, idxs)| idxs.len() >= 2)
        .map(|(command, entry_indices)| DuplicateGroup {
            command,
            entry_indices,
        })
        .collect();

    out.sort_by_key(|g| g.entry_indices[0]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::parse::parse;

    #[test]
    fn detects_identical_commands_after_trim() {
        let content = "ls\nGet-Process\n ls \nls\n";
        let entries = parse(content);
        let groups = find_duplicates(&entries);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].command, "ls");
        assert_eq!(groups[0].entry_indices, vec![0, 2, 3]);
    }

    #[test]
    fn case_differences_are_not_duplicates() {
        let content = "Get-Process\nget-process\n";
        let entries = parse(content);
        let groups = find_duplicates(&entries);
        assert!(groups.is_empty());
    }

    #[test]
    fn no_duplicates_returns_empty() {
        let content = "Get-Process\nGet-ChildItem\n";
        let entries = parse(content);
        assert!(find_duplicates(&entries).is_empty());
    }

    #[test]
    fn groups_sorted_by_first_occurrence() {
        let content = "b\na\nb\na\n";
        let entries = parse(content);
        let groups = find_duplicates(&entries);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].command, "b");
        assert_eq!(groups[1].command, "a");
    }
}
