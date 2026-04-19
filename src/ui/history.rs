use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap},
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::backup;
use crate::history::{
    dedup::{DuplicateGroup, find_duplicates},
    editplan::{Action, EditPlan},
    parse::{Entry, parse},
    typos::{DEFAULT_DENYLIST, TypoFlag, find_typos, high_confidence},
};
use crate::inventory::Inventory;
use crate::paths;

const TOAST_DURATION: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterTab {
    All,
    Typos,
    Duplicates,
}

impl FilterTab {
    const ORDER: [FilterTab; 3] = [FilterTab::All, FilterTab::Typos, FilterTab::Duplicates];

    fn next(self) -> Self {
        match self {
            FilterTab::All => FilterTab::Typos,
            FilterTab::Typos => FilterTab::Duplicates,
            FilterTab::Duplicates => FilterTab::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            FilterTab::All => "All",
            FilterTab::Typos => "Typos",
            FilterTab::Duplicates => "Duplicates",
        }
    }

    fn index(self) -> usize {
        match self {
            FilterTab::All => 0,
            FilterTab::Typos => 1,
            FilterTab::Duplicates => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Typo(usize),
    Duplicate(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenAction {
    None,
    Quit,
}

pub struct HistoryScreen {
    history_path: PathBuf,
    inventory: Inventory,
    file_present: bool,
    entries: Vec<Entry>,
    duplicate_groups: Vec<DuplicateGroup>,
    typo_flags: Vec<TypoFlag>,
    items: Vec<Item>,
    plan: EditPlan,
    filter: FilterTab,
    selected_visible: usize,
    preview_open: bool,
    confirm_apply: bool,
    toast: Option<(String, Instant)>,
}

impl HistoryScreen {
    pub fn new(
        history_path: PathBuf,
        inventory: Inventory,
        file_present: bool,
        entries: Vec<Entry>,
        duplicate_groups: Vec<DuplicateGroup>,
        typo_flags: Vec<TypoFlag>,
    ) -> Self {
        let items = Self::build_items(&typo_flags, &duplicate_groups);
        Self {
            history_path,
            inventory,
            file_present,
            entries,
            duplicate_groups,
            typo_flags,
            items,
            plan: EditPlan::new(),
            filter: FilterTab::All,
            selected_visible: 0,
            preview_open: false,
            confirm_apply: false,
            toast: None,
        }
    }

    pub fn pending_count(&self) -> usize {
        self.plan.pending_count()
    }

    fn reload_from_disk(&mut self) {
        let (content, present) = match std::fs::read_to_string(&self.history_path) {
            Ok(c) => (c, true),
            Err(_) => (String::new(), false),
        };
        self.entries = parse(&content);
        self.duplicate_groups = find_duplicates(&self.entries);
        self.typo_flags = find_typos(&self.entries, &self.inventory, DEFAULT_DENYLIST);
        self.items = Self::build_items(&self.typo_flags, &self.duplicate_groups);
        self.file_present = present;
        self.plan = EditPlan::new();
        self.selected_visible = 0;
    }

    fn build_items(typos: &[TypoFlag], dups: &[DuplicateGroup]) -> Vec<Item> {
        let mut keyed: Vec<(usize, Item)> = Vec::with_capacity(typos.len() + dups.len());
        for (i, flag) in typos.iter().enumerate() {
            keyed.push((flag.entry_index, Item::Typo(i)));
        }
        for (i, g) in dups.iter().enumerate() {
            keyed.push((g.entry_indices[0], Item::Duplicate(i)));
        }
        keyed.sort_by_key(|(k, _)| *k);
        keyed.into_iter().map(|(_, item)| item).collect()
    }

    fn visible_items(&self) -> Vec<Item> {
        self.items
            .iter()
            .filter(|item| {
                matches!(
                    (self.filter, item),
                    (FilterTab::All, _)
                        | (FilterTab::Typos, Item::Typo(_))
                        | (FilterTab::Duplicates, Item::Duplicate(_))
                )
            })
            .copied()
            .collect()
    }

    fn selected_item(&self) -> Option<Item> {
        self.visible_items().get(self.selected_visible).copied()
    }

    pub fn tick(&mut self) {
        if let Some((_, when)) = &self.toast
            && when.elapsed() > TOAST_DURATION
        {
            self.toast = None;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ScreenAction {
        if self.confirm_apply {
            return self.handle_confirm_key(key);
        }
        if self.preview_open {
            if matches!(
                key.code,
                KeyCode::Char('p') | KeyCode::Esc | KeyCode::Char('q')
            ) {
                self.preview_open = false;
            }
            return ScreenAction::None;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::F(10) => return ScreenAction::Quit,
            KeyCode::Tab => {
                self.filter = self.filter.next();
                self.selected_visible = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if self.apply_delete() {
                    self.advance_selection();
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if self.apply_replace() {
                    self.advance_selection();
                }
            }
            KeyCode::Char('K') => {
                if self.apply_keep() {
                    self.advance_selection();
                }
            }
            KeyCode::Char('c') => {
                if self.apply_collapse() {
                    self.advance_selection();
                }
            }
            KeyCode::Char('A') => self.bulk_autofix_typos(),
            KeyCode::Char('X') => self.bulk_collapse_all(),
            KeyCode::Char('p') => self.preview_open = true,
            KeyCode::F(5) => self.reload_with_dirty_check(),
            KeyCode::Char('o') => self.open_in_external_editor(),
            KeyCode::F(2) => self.request_apply(),
            _ => {}
        }
        ScreenAction::None
    }

    fn handle_confirm_key(&mut self, key: KeyEvent) -> ScreenAction {
        self.confirm_apply = false;
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => match self.do_apply() {
                Ok(ts) => {
                    self.reload_from_disk();
                    self.show_toast(format!("applied — backup {ts}"));
                }
                Err(e) => self.show_toast(format!("apply failed: {e}")),
            },
            _ => self.show_toast("apply cancelled".into()),
        }
        ScreenAction::None
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.visible_items().len();
        if len == 0 {
            self.selected_visible = 0;
            return;
        }
        let cur = self.selected_visible as i32;
        let next = (cur + delta).rem_euclid(len as i32);
        self.selected_visible = next as usize;
    }

    fn apply_delete(&mut self) -> bool {
        let Some(item) = self.selected_item() else {
            return false;
        };
        match item {
            Item::Typo(i) => {
                let idx = self.typo_flags[i].entry_index;
                self.plan.set(idx, Action::Delete);
            }
            Item::Duplicate(i) => {
                let indices = self.duplicate_groups[i].entry_indices.clone();
                for idx in indices {
                    self.plan.set(idx, Action::Delete);
                }
            }
        }
        true
    }

    fn apply_replace(&mut self) -> bool {
        if let Some(Item::Typo(i)) = self.selected_item()
            && let Some(sugg) = self.typo_flags[i].suggestion.clone()
        {
            let idx = self.typo_flags[i].entry_index;
            self.plan.set(idx, Action::Replace(sugg));
            return true;
        }
        false
    }

    fn apply_keep(&mut self) -> bool {
        let Some(item) = self.selected_item() else {
            return false;
        };
        match item {
            Item::Typo(i) => {
                let idx = self.typo_flags[i].entry_index;
                self.plan.set(idx, Action::Keep);
            }
            Item::Duplicate(i) => {
                let indices = self.duplicate_groups[i].entry_indices.clone();
                for idx in indices {
                    self.plan.set(idx, Action::Keep);
                }
            }
        }
        true
    }

    fn apply_collapse(&mut self) -> bool {
        if let Some(Item::Duplicate(i)) = self.selected_item() {
            let group = self.duplicate_groups[i].clone();
            collapse_group(&mut self.plan, &group);
            return true;
        }
        false
    }

    fn advance_selection(&mut self) {
        let len = self.visible_items().len();
        if len <= 1 {
            return;
        }
        if self.selected_visible + 1 < len {
            self.selected_visible += 1;
        }
    }

    fn bulk_autofix_typos(&mut self) {
        let flags: Vec<TypoFlag> = self.typo_flags.clone();
        for flag in flags {
            if high_confidence(&flag)
                && let Some(sugg) = flag.suggestion
            {
                self.plan.set(flag.entry_index, Action::Replace(sugg));
            }
        }
    }

    fn bulk_collapse_all(&mut self) {
        let groups: Vec<DuplicateGroup> = self.duplicate_groups.clone();
        for g in &groups {
            collapse_group(&mut self.plan, g);
        }
    }

    fn reload_with_dirty_check(&mut self) {
        if !self.plan.is_empty() {
            self.show_toast("unsaved changes — apply or discard first".into());
            return;
        }
        self.reload_from_disk();
        self.show_toast("reloaded from disk".into());
    }

    fn open_in_external_editor(&mut self) {
        if !self.file_present {
            self.show_toast("no history file to open".into());
            return;
        }
        match launch_editor(&self.history_path) {
            Ok(_) => self.show_toast("opened in default editor — F5 to reload".into()),
            Err(e) => self.show_toast(format!("launch failed: {e}")),
        }
    }

    fn request_apply(&mut self) {
        if !self.file_present {
            self.show_toast("no history file to write".into());
            return;
        }
        if self.plan.is_empty() {
            self.show_toast("no changes to apply".into());
            return;
        }
        self.confirm_apply = true;
    }

    fn do_apply(&self) -> Result<String> {
        let backups_root = paths::backups_dir()?;
        let backup = backup::create(&backups_root, &[self.history_path.as_path()])?;
        let rendered = self.plan.render(&self.entries);
        backup::atomic_write_str(&self.history_path, &rendered)?;
        Ok(backup.timestamp)
    }

    fn show_toast(&mut self, msg: String) {
        self.toast = Some((msg, Instant::now()));
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
            ])
            .split(area);

        self.render_header(f, chunks[0]);
        self.render_tabs(f, chunks[1]);
        self.render_body(f, chunks[2]);
        self.render_status(f, chunks[3]);

        if self.preview_open {
            self.render_preview_modal(f, area);
        }
        if self.confirm_apply {
            self.render_confirm_modal(f, area);
        }
        if let Some((msg, _)) = &self.toast {
            render_toast(f, area, msg);
        }
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let file_line = format!("File: {}", self.history_path.display());
        let stats = if self.file_present {
            format!(
                "{} lines │ {} duplicate groups │ {} suspected typos │ pending: {}",
                self.entries.len(),
                self.duplicate_groups.len(),
                self.typo_flags.len(),
                self.plan.pending_count()
            )
        } else {
            "no history file found — read-only".into()
        };
        let text = vec![Line::from(file_line), Line::from(stats)];
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" powershellknife — History ");
        let p = Paragraph::new(text).block(block);
        f.render_widget(p, area);
    }

    fn render_tabs(&self, f: &mut Frame, area: Rect) {
        let titles: Vec<Line> = FilterTab::ORDER
            .iter()
            .map(|t| {
                let count = match t {
                    FilterTab::All => self.items.len(),
                    FilterTab::Typos => self.typo_flags.len(),
                    FilterTab::Duplicates => self.duplicate_groups.len(),
                };
                Line::from(format!(" {} ({}) ", t.label(), count))
            })
            .collect();
        let tabs = Tabs::new(titles)
            .select(self.filter.index())
            .block(Block::default().borders(Borders::ALL))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_widget(tabs, area);
    }

    fn render_body(&self, f: &mut Frame, area: Rect) {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);
        self.render_list(f, split[0]);
        self.render_detail(f, split[1]);
    }

    fn render_list(&self, f: &mut Frame, area: Rect) {
        let items = self.visible_items();
        let rows: Vec<ListItem> = items
            .iter()
            .map(|item| self.render_list_item(item))
            .collect();
        let list = List::new(rows)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Flagged entries "),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut state = ListState::default();
        if !items.is_empty() {
            let sel = self.selected_visible.min(items.len() - 1);
            state.select(Some(sel));
        }
        f.render_stateful_widget(list, area, &mut state);
    }

    fn render_list_item(&self, item: &Item) -> ListItem<'static> {
        match item {
            Item::Typo(i) => {
                let flag = &self.typo_flags[*i];
                let entry = &self.entries[flag.entry_index];
                let marker = action_marker(self.plan.get(flag.entry_index));
                let text = format!(
                    "{} [typo] L{:<5} {}",
                    marker,
                    entry.start_line,
                    truncate(&entry.command, 60),
                );
                ListItem::new(text)
            }
            Item::Duplicate(i) => {
                let g = &self.duplicate_groups[*i];
                let first_entry = &self.entries[g.entry_indices[0]];
                let marker = action_marker(self.plan.get(g.entry_indices[0]));
                let text = format!(
                    "{} [dup]  L{:<5} {}  (x{})",
                    marker,
                    first_entry.start_line,
                    truncate(&g.command, 50),
                    g.entry_indices.len(),
                );
                ListItem::new(text)
            }
        }
    }

    fn render_detail(&self, f: &mut Frame, area: Rect) {
        let text: Vec<Line> = match self.selected_item() {
            None => vec![Line::from("no entries flagged")],
            Some(Item::Typo(i)) => self.detail_typo_lines(i),
            Some(Item::Duplicate(i)) => self.detail_duplicate_lines(i),
        };
        let p = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" Detail "))
            .wrap(Wrap { trim: false });
        f.render_widget(p, area);
    }

    fn detail_typo_lines(&self, i: usize) -> Vec<Line<'static>> {
        let flag = &self.typo_flags[i];
        let entry = &self.entries[flag.entry_index];
        let mut lines = vec![
            Line::from(Span::styled(
                "Typo",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(format!("line {}: {}", entry.start_line, entry.command)),
            Line::from(format!("token: {}", flag.first_token)),
        ];
        match &flag.suggestion {
            Some(s) => lines.push(Line::from(format!("suggestion: {s}"))),
            None => lines.push(Line::from("no suggestion close enough")),
        }
        lines.push(Line::from(""));
        lines.push(Line::from(format!(
            "action: {}",
            describe_action(self.plan.get(flag.entry_index))
        )));
        lines.push(Line::from(""));
        lines.push(Line::from("[d] delete  [r] replace  [K] keep"));
        lines
    }

    fn detail_duplicate_lines(&self, i: usize) -> Vec<Line<'static>> {
        let g = &self.duplicate_groups[i];
        let lines_list: Vec<String> = g
            .entry_indices
            .iter()
            .map(|&idx| self.entries[idx].start_line.to_string())
            .collect();
        let mut lines = vec![
            Line::from(Span::styled(
                "Duplicate",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(format!("command: {}", g.command)),
            Line::from(format!(
                "occurs {} times at lines: {}",
                g.entry_indices.len(),
                lines_list.join(", "),
            )),
            Line::from(""),
        ];
        let last_rank = g.entry_indices.len().saturating_sub(1);
        for (rank, &idx) in g.entry_indices.iter().enumerate() {
            let tag = if rank == last_rank {
                "latest "
            } else {
                "earlier"
            };
            lines.push(Line::from(format!(
                "  {} L{:<5}  [{}]",
                tag,
                self.entries[idx].start_line,
                describe_action(self.plan.get(idx))
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from("[c] collapse  [d] delete all  [K] keep all"));
        lines
    }

    fn render_status(&self, f: &mut Frame, area: Rect) {
        let text = "[A] auto-fix  [X] collapse dups  [p] preview  [F2] apply  [o] editor  [F5] reload  [Tab] filter  [q] quit";
        let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
        f.render_widget(p, area);
    }

    fn render_preview_modal(&self, f: &mut Frame, area: Rect) {
        let text = if self.plan.is_empty() {
            "(no pending changes)".to_string()
        } else {
            self.plan.preview(&self.entries)
        };
        let modal = centered(area, 80, 70);
        f.render_widget(Clear, modal);
        let p = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Preview — p or Esc to close "),
            )
            .wrap(Wrap { trim: false });
        f.render_widget(p, modal);
    }

    fn render_confirm_modal(&self, f: &mut Frame, area: Rect) {
        let modal = centered(area, 60, 30);
        f.render_widget(Clear, modal);
        let text = format!(
            "Apply {} pending change(s) to\n{}?\n\n[y] confirm    any other key cancels",
            self.plan.pending_count(),
            self.history_path.display()
        );
        let p = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Confirm apply "),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false });
        f.render_widget(p, modal);
    }
}

fn render_toast(f: &mut Frame, area: Rect, msg: &str) {
    let width = (msg.chars().count() as u16 + 6).min(area.width.saturating_sub(4));
    let x = area.x + area.width.saturating_sub(width + 2);
    let y = area.y + area.height.saturating_sub(4);
    let rect = Rect {
        x,
        y,
        width,
        height: 3,
    };
    f.render_widget(Clear, rect);
    let p = Paragraph::new(msg)
        .block(Block::default().borders(Borders::ALL).title(" info "))
        .alignment(Alignment::Center);
    f.render_widget(p, rect);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn describe_action(a: &Action) -> String {
    match a {
        Action::Keep => "keep".to_string(),
        Action::Delete => "delete".to_string(),
        Action::Replace(s) => format!("replace → {s}"),
    }
}

fn action_marker(a: &Action) -> char {
    match a {
        Action::Keep => ' ',
        Action::Delete => '-',
        Action::Replace(_) => 'R',
    }
}

#[cfg(windows)]
fn launch_editor(path: &Path) -> Result<()> {
    Command::new("cmd")
        .args(["/C", "start", "", path.to_string_lossy().as_ref()])
        .spawn()
        .with_context(|| format!("spawning cmd start for {}", path.display()))?;
    Ok(())
}

#[cfg(not(windows))]
fn launch_editor(path: &Path) -> Result<()> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "xdg-open".to_string());
    Command::new(&editor)
        .arg(path)
        .spawn()
        .with_context(|| format!("spawning {editor} for {}", path.display()))?;
    Ok(())
}

fn collapse_group(plan: &mut EditPlan, g: &DuplicateGroup) {
    let Some(&latest) = g.entry_indices.last() else {
        return;
    };
    for &idx in &g.entry_indices {
        if idx == latest {
            plan.set(idx, Action::Keep);
        } else {
            plan.set(idx, Action::Delete);
        }
    }
}

fn centered(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage((100 - height_pct) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{
        dedup::find_duplicates,
        parse::parse,
        typos::{DEFAULT_DENYLIST, find_typos},
    };
    use crate::inventory::Inventory;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use std::path::PathBuf;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_shift(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)
    }

    fn build_screen(content: &str) -> HistoryScreen {
        let entries = parse(content);
        let dups = find_duplicates(&entries);
        let inv = Inventory::embedded().unwrap();
        let typos = find_typos(&entries, &inv, DEFAULT_DENYLIST);
        HistoryScreen::new(PathBuf::from("dummy.txt"), inv, true, entries, dups, typos)
    }

    #[test]
    fn items_are_sorted_by_first_line() {
        let screen = build_screen("Get-Procss\nls\nls\ndocer ps\nls\n");
        assert!(screen.items.len() >= 3);
        // The 'Get-Procss' typo starts at entry 0, 'ls' dup at 1, 'docer' typo at 3.
        // Items should be ordered by line index ascending.
        let order: Vec<_> = screen
            .items
            .iter()
            .map(|it| match it {
                Item::Typo(i) => screen.typo_flags[*i].entry_index,
                Item::Duplicate(i) => screen.duplicate_groups[*i].entry_indices[0],
            })
            .collect();
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(order, sorted);
    }

    #[test]
    fn tab_cycles_filter_and_resets_selection() {
        let mut screen = build_screen("Get-Procss\nls\nls\n");
        assert_eq!(screen.filter, FilterTab::All);
        screen.handle_key(key(KeyCode::Down));
        assert_eq!(
            screen.selected_visible,
            1.min(screen.visible_items().len() - 1)
        );

        screen.handle_key(key(KeyCode::Tab));
        assert_eq!(screen.filter, FilterTab::Typos);
        assert_eq!(screen.selected_visible, 0);

        screen.handle_key(key(KeyCode::Tab));
        assert_eq!(screen.filter, FilterTab::Duplicates);
        screen.handle_key(key(KeyCode::Tab));
        assert_eq!(screen.filter, FilterTab::All);
    }

    #[test]
    fn navigation_wraps_around() {
        let mut screen = build_screen("Get-Procss\nls\nls\n");
        let len = screen.visible_items().len();
        assert!(len >= 2);
        screen.selected_visible = 0;
        screen.handle_key(key(KeyCode::Up));
        assert_eq!(screen.selected_visible, len - 1);
        screen.handle_key(key(KeyCode::Down));
        assert_eq!(screen.selected_visible, 0);
    }

    #[test]
    fn delete_marks_selected_typo() {
        let mut screen = build_screen("Get-Procss -Name chrome\n");
        screen.selected_visible = 0;
        screen.handle_key(key(KeyCode::Char('d')));
        assert_eq!(screen.plan.pending_count(), 1);
        assert!(matches!(screen.plan.get(0), Action::Delete));
    }

    #[test]
    fn replace_uses_suggestion_only_for_typos() {
        let mut screen = build_screen("Get-Procss -Name chrome\n");
        screen.selected_visible = 0;
        screen.handle_key(key(KeyCode::Char('r')));
        assert!(matches!(screen.plan.get(0), Action::Replace(s) if s == "Get-Process"));
    }

    #[test]
    fn replace_noop_when_no_suggestion() {
        let mut screen = build_screen("docer ps\n");
        screen.selected_visible = 0;
        screen.handle_key(key(KeyCode::Char('r')));
        assert!(screen.plan.is_empty());
    }

    #[test]
    fn keep_clears_previous_action() {
        let mut screen = build_screen("Get-Procss\n");
        screen.selected_visible = 0;
        screen.handle_key(key(KeyCode::Char('d')));
        assert_eq!(screen.plan.pending_count(), 1);
        screen.handle_key(key_shift('K'));
        assert!(screen.plan.is_empty());
    }

    #[test]
    fn collapse_keeps_latest_deletes_earlier() {
        let mut screen = build_screen("ls\nls\nls\n");
        screen.filter = FilterTab::Duplicates;
        screen.selected_visible = 0;
        screen.handle_key(key(KeyCode::Char('c')));
        assert!(matches!(screen.plan.get(0), Action::Delete));
        assert!(matches!(screen.plan.get(1), Action::Delete));
        assert!(matches!(screen.plan.get(2), Action::Keep));
    }

    #[test]
    fn collapse_noop_on_typo_item() {
        let mut screen = build_screen("Get-Procss\n");
        screen.filter = FilterTab::Typos;
        screen.selected_visible = 0;
        screen.handle_key(key(KeyCode::Char('c')));
        assert!(screen.plan.is_empty());
    }

    #[test]
    fn autofix_replaces_high_confidence_typos() {
        let mut screen = build_screen("Get-Procss\ndocer ps\n");
        screen.handle_key(key_shift('A'));
        assert!(matches!(screen.plan.get(0), Action::Replace(_)));
        assert!(matches!(screen.plan.get(1), Action::Keep));
    }

    #[test]
    fn bulk_collapse_covers_every_group() {
        let mut screen = build_screen("ls\nls\nGet-Process\nGet-Process\n");
        assert_eq!(screen.duplicate_groups.len(), 2);
        screen.handle_key(key_shift('X'));
        assert!(matches!(screen.plan.get(0), Action::Delete));
        assert!(matches!(screen.plan.get(1), Action::Keep));
        assert!(matches!(screen.plan.get(2), Action::Delete));
        assert!(matches!(screen.plan.get(3), Action::Keep));
    }

    #[test]
    fn preview_toggles_open_and_close() {
        let mut screen = build_screen("Get-Procss\n");
        assert!(!screen.preview_open);
        screen.handle_key(key(KeyCode::Char('p')));
        assert!(screen.preview_open);
        screen.handle_key(key(KeyCode::Esc));
        assert!(!screen.preview_open);
    }

    #[test]
    fn apply_without_changes_shows_toast_not_confirm() {
        let mut screen = build_screen("Get-Procss\n");
        screen.handle_key(key(KeyCode::F(2)));
        assert!(!screen.confirm_apply);
        assert!(screen.toast.is_some());
    }

    #[test]
    fn apply_with_changes_prompts_confirmation() {
        let mut screen = build_screen("Get-Procss\n");
        screen.selected_visible = 0;
        screen.handle_key(key(KeyCode::Char('d')));
        screen.handle_key(key(KeyCode::F(2)));
        assert!(screen.confirm_apply);
    }

    #[test]
    fn apply_rejected_when_file_missing() {
        let mut screen = build_screen("Get-Procss\n");
        screen.file_present = false;
        screen.selected_visible = 0;
        screen.handle_key(key(KeyCode::Char('d')));
        screen.handle_key(key(KeyCode::F(2)));
        assert!(!screen.confirm_apply);
        assert!(screen.toast.is_some());
    }

    #[test]
    fn cancel_confirmation_with_any_other_key() {
        let mut screen = build_screen("Get-Procss\n");
        screen.selected_visible = 0;
        screen.handle_key(key(KeyCode::Char('d')));
        screen.handle_key(key(KeyCode::F(2)));
        assert!(screen.confirm_apply);
        screen.handle_key(key(KeyCode::Char('n')));
        assert!(!screen.confirm_apply);
    }

    #[test]
    fn delete_advances_selection_to_next_item() {
        let mut screen = build_screen("Get-Procss\ndocer ps\nzadir cmd\n");
        assert!(screen.visible_items().len() >= 2);
        screen.selected_visible = 0;
        screen.handle_key(key(KeyCode::Char('d')));
        assert_eq!(screen.selected_visible, 1);
        screen.handle_key(key(KeyCode::Char('d')));
        assert_eq!(screen.selected_visible, 2);
    }

    #[test]
    fn advance_does_not_wrap_past_last_item() {
        let mut screen = build_screen("Get-Procss\ndocer ps\n");
        let last = screen.visible_items().len() - 1;
        screen.selected_visible = last;
        screen.handle_key(key(KeyCode::Char('d')));
        assert_eq!(screen.selected_visible, last);
    }

    #[test]
    fn replace_without_suggestion_does_not_advance() {
        let mut screen = build_screen("docer ps\nGet-Procss\n");
        screen.selected_visible = 0;
        screen.handle_key(key(KeyCode::Char('r')));
        assert_eq!(screen.selected_visible, 0);
    }

    #[test]
    fn quit_returns_quit_action() {
        let mut screen = build_screen("Get-Procss\n");
        let action = screen.handle_key(key(KeyCode::Char('q')));
        assert_eq!(action, ScreenAction::Quit);
    }

    #[test]
    fn truncate_shortens_long_strings() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("helloworld!", 5).chars().count(), 5);
    }
}
