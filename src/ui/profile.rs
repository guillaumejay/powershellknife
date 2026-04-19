use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::backup;
use crate::paths;
use crate::profile::block::{self, BlockState};
use crate::profile::settings::{BellStyle, EditMode, PredictionSource, Settings};

const TOAST_DURATION: Duration = Duration::from_secs(3);
const PSREADLINE_ROW_COUNT: usize = 5;

const PSREADLINE_DOCS_BASE: &str =
    "https://learn.microsoft.com/en-us/powershell/module/psreadline/set-psreadlineoption";

const PSREADLINE_DOC_ANCHORS: [&str; PSREADLINE_ROW_COUNT] = [
    "#-historynoduplicates",
    "#-historysearchcursormovestoend",
    "#-predictionsource",
    "#-editmode",
    "#-bellstyle",
];

fn psreadline_docs_url(row: usize) -> String {
    let anchor = PSREADLINE_DOC_ANCHORS.get(row).copied().unwrap_or_default();
    format!("{PSREADLINE_DOCS_BASE}{anchor}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    PSReadLine,
    Modules,
    Aliases,
    Custom,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Focus::PSReadLine => Focus::Modules,
            Focus::Modules => Focus::Aliases,
            Focus::Aliases => Focus::Custom,
            Focus::Custom => Focus::PSReadLine,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PromptKind {
    AddModule,
    AddAliasName,
    AddAliasValue { name: String },
    EditAliasValue { name: String },
}

#[derive(Debug, Clone)]
struct Prompt {
    kind: PromptKind,
    buffer: String,
    title: String,
    hint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenAction {
    None,
    Quit,
}

pub struct ProfileScreen {
    profile_path: PathBuf,
    file_present: bool,
    original_content: String,
    corrupted_error: Option<String>,
    settings: Settings,
    baseline: Settings,
    custom_prefix: String,
    custom_suffix: String,
    focus: Focus,
    psreadline_row: usize,
    modules_row: usize,
    aliases_row: usize,
    custom_expanded: bool,
    prompt: Option<Prompt>,
    confirm_apply: bool,
    toast: Option<(String, Instant)>,
}

impl ProfileScreen {
    pub fn new(profile_path: PathBuf) -> Self {
        let mut screen = Self {
            profile_path,
            file_present: false,
            original_content: String::new(),
            corrupted_error: None,
            settings: Settings::default(),
            baseline: Settings::default(),
            custom_prefix: String::new(),
            custom_suffix: String::new(),
            focus: Focus::PSReadLine,
            psreadline_row: 0,
            modules_row: 0,
            aliases_row: 0,
            custom_expanded: false,
            prompt: None,
            confirm_apply: false,
            toast: None,
        };
        screen.reload_from_disk();
        screen
    }

    fn reload_from_disk(&mut self) {
        let (content, present) = match std::fs::read_to_string(&self.profile_path) {
            Ok(c) => (c, true),
            Err(_) => (String::new(), false),
        };
        self.file_present = present;
        self.original_content = content.clone();
        self.corrupted_error = None;
        self.custom_prefix = String::new();
        self.custom_suffix = String::new();

        match block::locate(&content) {
            BlockState::Present(slice) => {
                self.settings = Settings::parse(&slice.inner_lines);
                self.custom_prefix = slice.prefix;
                self.custom_suffix = slice.suffix;
            }
            BlockState::Missing => {
                self.settings = Settings::default();
                self.custom_prefix = content;
            }
            BlockState::Corrupted(msg) => {
                self.settings = Settings::default();
                self.corrupted_error = Some(msg);
            }
        }
        self.baseline = self.settings.clone();
        self.psreadline_row = 0;
        self.modules_row = 0;
        self.aliases_row = 0;
        self.prompt = None;
        self.confirm_apply = false;
    }

    pub fn is_dirty(&self) -> bool {
        self.corrupted_error.is_none() && self.settings != self.baseline
    }

    pub fn tick(&mut self) {
        if let Some((_, when)) = &self.toast
            && when.elapsed() > TOAST_DURATION
        {
            self.toast = None;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ScreenAction {
        if self.corrupted_error.is_some() {
            match key.code {
                KeyCode::Char('q') | KeyCode::F(10) => return ScreenAction::Quit,
                KeyCode::Char('o') => self.open_in_external_editor(),
                KeyCode::F(5) => self.reload_from_disk(),
                _ => {}
            }
            return ScreenAction::None;
        }
        if self.confirm_apply {
            return self.handle_confirm_key(key);
        }
        if self.prompt.is_some() {
            self.handle_prompt_key(key);
            return ScreenAction::None;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::F(10) => return ScreenAction::Quit,
            KeyCode::Tab => {
                self.focus = self.focus.next();
            }
            KeyCode::F(2) => self.request_apply(),
            KeyCode::Char('o') => self.open_in_external_editor(),
            KeyCode::F(5) => self.reload_with_dirty_check(),
            _ => match self.focus {
                Focus::PSReadLine => self.handle_psreadline_key(key),
                Focus::Modules => self.handle_modules_key(key),
                Focus::Aliases => self.handle_aliases_key(key),
                Focus::Custom => self.handle_custom_key(key),
            },
        }
        ScreenAction::None
    }

    fn reload_with_dirty_check(&mut self) {
        if self.is_dirty() {
            self.show_toast("unsaved changes — apply or discard first".into());
            return;
        }
        self.reload_from_disk();
        self.show_toast("reloaded from disk".into());
    }

    fn open_in_external_editor(&mut self) {
        if let Err(e) = ensure_file_exists(&self.profile_path) {
            self.show_toast(format!("cannot create file: {e}"));
            return;
        }
        match launch_editor(&self.profile_path) {
            Ok(_) => self.show_toast("opened in default editor — F5 to reload".into()),
            Err(e) => self.show_toast(format!("launch failed: {e}")),
        }
    }

    fn handle_psreadline_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.psreadline_row > 0 {
                    self.psreadline_row -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.psreadline_row + 1 < PSREADLINE_ROW_COUNT {
                    self.psreadline_row += 1;
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_or_cycle(1),
            KeyCode::Left => self.toggle_or_cycle(-1),
            KeyCode::Right => self.toggle_or_cycle(1),
            KeyCode::Char('?') => self.open_docs_for_current_option(),
            _ => {}
        }
    }

    fn open_docs_for_current_option(&mut self) {
        let url = psreadline_docs_url(self.psreadline_row);
        match launch_external(&url) {
            Ok(_) => self.show_toast("opened docs in browser".into()),
            Err(e) => self.show_toast(format!("launch failed: {e}")),
        }
    }

    fn toggle_or_cycle(&mut self, direction: i32) {
        match self.psreadline_row {
            0 => {
                self.settings.psreadline.history_no_duplicates =
                    !self.settings.psreadline.history_no_duplicates;
            }
            1 => {
                self.settings.psreadline.history_search_cursor_moves_to_end =
                    !self.settings.psreadline.history_search_cursor_moves_to_end;
            }
            2 => {
                self.settings.psreadline.prediction_source = cycle_enum(
                    PredictionSource::ALL,
                    self.settings.psreadline.prediction_source,
                    direction,
                );
            }
            3 => {
                self.settings.psreadline.edit_mode =
                    cycle_enum(EditMode::ALL, self.settings.psreadline.edit_mode, direction);
            }
            4 => {
                self.settings.psreadline.bell_style = cycle_enum(
                    BellStyle::ALL,
                    self.settings.psreadline.bell_style,
                    direction,
                );
            }
            _ => {}
        }
    }

    fn handle_modules_key(&mut self, key: KeyEvent) {
        let total = self.settings.modules.len() + 1;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.modules_row > 0 {
                    self.modules_row -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.modules_row + 1 < total {
                    self.modules_row += 1;
                }
            }
            KeyCode::Enter => {
                if self.modules_row == self.settings.modules.len() {
                    self.prompt = Some(Prompt {
                        kind: PromptKind::AddModule,
                        buffer: String::new(),
                        title: "Add module".to_string(),
                        hint: "module name (e.g. posh-git)".to_string(),
                    });
                }
            }
            KeyCode::Delete | KeyCode::Char('x') => {
                if self.modules_row < self.settings.modules.len() {
                    let removed = self.settings.modules.remove(self.modules_row);
                    self.show_toast(format!("removed module {removed}"));
                    if self.modules_row >= self.settings.modules.len() && self.modules_row > 0 {
                        self.modules_row -= 1;
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_aliases_key(&mut self, key: KeyEvent) {
        let alias_keys: Vec<String> = self.settings.aliases.keys().cloned().collect();
        let total = alias_keys.len() + 1;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.aliases_row > 0 {
                    self.aliases_row -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.aliases_row + 1 < total {
                    self.aliases_row += 1;
                }
            }
            KeyCode::Enter => {
                if self.aliases_row == alias_keys.len() {
                    self.prompt = Some(Prompt {
                        kind: PromptKind::AddAliasName,
                        buffer: String::new(),
                        title: "Add alias — name".to_string(),
                        hint: "alias name (e.g. ll)".to_string(),
                    });
                }
            }
            KeyCode::Char('e') => {
                if let Some(name) = alias_keys.get(self.aliases_row) {
                    let current = self.settings.aliases.get(name).cloned().unwrap_or_default();
                    self.prompt = Some(Prompt {
                        kind: PromptKind::EditAliasValue { name: name.clone() },
                        buffer: current,
                        title: format!("Edit alias — {name}"),
                        hint: "new value".to_string(),
                    });
                }
            }
            KeyCode::Delete | KeyCode::Char('x') => {
                if let Some(name) = alias_keys.get(self.aliases_row) {
                    let name = name.clone();
                    self.settings.aliases.remove(&name);
                    self.show_toast(format!("removed alias {name}"));
                    let new_total = self.settings.aliases.len();
                    if self.aliases_row >= new_total && self.aliases_row > 0 {
                        self.aliases_row -= 1;
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_custom_key(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
            self.custom_expanded = !self.custom_expanded;
        }
    }

    fn handle_prompt_key(&mut self, key: KeyEvent) {
        let Some(prompt) = self.prompt.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.prompt = None;
            }
            KeyCode::Backspace => {
                prompt.buffer.pop();
            }
            KeyCode::Enter => {
                let value = prompt.buffer.trim().to_string();
                let kind = prompt.kind.clone();
                self.prompt = None;
                self.commit_prompt(kind, value);
            }
            KeyCode::Char(c) => {
                prompt.buffer.push(c);
            }
            _ => {}
        }
    }

    fn commit_prompt(&mut self, kind: PromptKind, value: String) {
        if value.is_empty() {
            self.show_toast("empty input — cancelled".into());
            return;
        }
        match kind {
            PromptKind::AddModule => {
                if self.settings.modules.iter().any(|m| m == &value) {
                    self.show_toast(format!("module {value} already present"));
                    return;
                }
                self.settings.modules.push(value.clone());
                self.modules_row = self.settings.modules.len() - 1;
                self.show_toast(format!("added module {value}"));
            }
            PromptKind::AddAliasName => {
                if self.settings.aliases.contains_key(&value) {
                    self.show_toast(format!("alias {value} already exists — use [e] to edit"));
                    return;
                }
                self.prompt = Some(Prompt {
                    kind: PromptKind::AddAliasValue {
                        name: value.clone(),
                    },
                    buffer: String::new(),
                    title: format!("Add alias — value for {value}"),
                    hint: "value (e.g. Get-ChildItem -Force)".to_string(),
                });
            }
            PromptKind::AddAliasValue { name } => {
                self.settings.aliases.insert(name.clone(), value);
                let pos = self
                    .settings
                    .aliases
                    .keys()
                    .position(|k| k == &name)
                    .unwrap_or(0);
                self.aliases_row = pos;
                self.show_toast(format!("added alias {name}"));
            }
            PromptKind::EditAliasValue { name } => {
                self.settings.aliases.insert(name.clone(), value);
                self.show_toast(format!("updated alias {name}"));
            }
        }
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

    fn request_apply(&mut self) {
        if !self.is_dirty() {
            self.show_toast("no changes to apply".into());
            return;
        }
        self.confirm_apply = true;
    }

    fn do_apply(&self) -> Result<String> {
        let backups_root = paths::backups_dir()?;
        let backup = backup::create(&backups_root, &[self.profile_path.as_path()])?;
        let new_content = block::compose(&self.original_content, &self.settings.serialize())?;
        backup::atomic_write_str(&self.profile_path, &new_content)?;
        Ok(backup.timestamp)
    }

    fn show_toast(&mut self, msg: String) {
        self.toast = Some((msg, Instant::now()));
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
            ])
            .split(area);

        self.render_header(f, chunks[0]);
        if let Some(msg) = &self.corrupted_error {
            self.render_corrupted(f, chunks[1], msg);
        } else {
            self.render_body(f, chunks[1]);
        }
        self.render_status(f, chunks[2]);

        if let Some(prompt) = &self.prompt {
            render_prompt(f, area, prompt);
        }
        if self.confirm_apply {
            self.render_confirm_modal(f, area);
        }
        if let Some((msg, _)) = &self.toast {
            render_toast(f, area, msg);
        }
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let dirty_tag = if self.is_dirty() { "dirty" } else { "clean" };
        let presence = if self.file_present {
            ""
        } else {
            " (file absent — will be created on apply)"
        };
        let text = vec![
            Line::from(format!("Profile: {}", self.profile_path.display())),
            Line::from(format!("state: {dirty_tag}{presence}")),
        ];
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" powershellknife — Profile ");
        f.render_widget(Paragraph::new(text).block(block), area);
    }

    fn render_corrupted(&self, f: &mut Frame, area: Rect, msg: &str) {
        let lines = vec![
            Line::from(Span::styled(
                "Managed block is corrupted — editing disabled",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(msg.to_string()),
            Line::from(""),
            Line::from(format!(
                "Fix the block in {} manually, then press [F5] to reload.",
                self.profile_path.display()
            )),
            Line::from("Press [o] to open the file in your default editor."),
            Line::from(""),
            Line::from(format!(
                "Markers expected:\n  {}\n  {}",
                block::START_MARKER,
                block::END_MARKER
            )),
        ];
        f.render_widget(
            Paragraph::new(lines)
                .block(Block::default().borders(Borders::ALL).title(" Error "))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_body(&self, f: &mut Frame, area: Rect) {
        let lines = self.body_lines();
        let block = Block::default().borders(Borders::ALL).title(" Settings ");
        f.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn body_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.extend(self.psreadline_lines());
        lines.push(Line::from(""));
        lines.extend(self.modules_lines());
        lines.push(Line::from(""));
        lines.extend(self.aliases_lines());
        lines.push(Line::from(""));
        lines.extend(self.custom_lines());
        lines
    }

    fn psreadline_lines(&self) -> Vec<Line<'static>> {
        let focused = self.focus == Focus::PSReadLine;
        let rows = [
            format!(
                "[{}] HistoryNoDuplicates",
                check(self.settings.psreadline.history_no_duplicates)
            ),
            format!(
                "[{}] HistorySearchCursorMovesToEnd",
                check(self.settings.psreadline.history_search_cursor_moves_to_end)
            ),
            format!(
                "PredictionSource    ‹ {:<17} ›",
                self.settings.psreadline.prediction_source.as_str()
            ),
            format!(
                "EditMode            ‹ {:<17} ›",
                self.settings.psreadline.edit_mode.as_str()
            ),
            format!(
                "BellStyle           ‹ {:<17} ›",
                self.settings.psreadline.bell_style.as_str()
            ),
        ];
        let mut out = vec![section_header("PSReadLine", focused)];
        for (i, text) in rows.iter().enumerate() {
            let selected = focused && i == self.psreadline_row;
            out.push(row_line(text, selected));
        }
        if focused {
            out.push(Line::from(Span::styled(
                format!(
                    "    docs ([?] open): {}",
                    psreadline_docs_url(self.psreadline_row)
                ),
                Style::default().fg(Color::DarkGray),
            )));
        }
        out
    }

    fn modules_lines(&self) -> Vec<Line<'static>> {
        let focused = self.focus == Focus::Modules;
        let mut out = vec![section_header("Modules auto-importés", focused)];
        for (i, module) in self.settings.modules.iter().enumerate() {
            let selected = focused && i == self.modules_row;
            out.push(row_line(module, selected));
        }
        let add_selected = focused && self.modules_row == self.settings.modules.len();
        out.push(row_line("[+] Add module…", add_selected));
        out
    }

    fn aliases_lines(&self) -> Vec<Line<'static>> {
        let focused = self.focus == Focus::Aliases;
        let mut out = vec![section_header("Aliases persistants", focused)];
        let entries: Vec<(String, String)> = self
            .settings
            .aliases
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (i, (name, value)) in entries.iter().enumerate() {
            let selected = focused && i == self.aliases_row;
            out.push(row_line(&format!("{name}  →  {value}"), selected));
        }
        let add_selected = focused && self.aliases_row == entries.len();
        out.push(row_line("[+] Add alias…", add_selected));
        out
    }

    fn custom_lines(&self) -> Vec<Line<'static>> {
        let focused = self.focus == Focus::Custom;
        let toggle = if self.custom_expanded { "[v]" } else { "[>]" };
        let header_text = format!("Custom profile code (preserved, read-only) {toggle}");
        let mut out = vec![section_header(&header_text, focused)];
        if self.custom_expanded {
            let combined = format!(
                "--- before block ---\n{}\n--- after block ---\n{}",
                self.custom_prefix, self.custom_suffix
            );
            for line in combined.lines().take(20) {
                out.push(Line::from(Span::styled(
                    format!("  {line}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            let total_lines = combined.lines().count();
            if total_lines > 20 {
                out.push(Line::from(Span::styled(
                    format!("  … ({} more lines)", total_lines - 20),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
        out
    }

    fn render_status(&self, f: &mut Frame, area: Rect) {
        if self.corrupted_error.is_some() {
            let text = "[o] open in editor  [F5] reload  [q] quit";
            f.render_widget(
                Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
                area,
            );
            return;
        }
        let text = match self.focus {
            Focus::PSReadLine => {
                "[↑↓] row  [Space/←→] toggle  [?] docs  [Tab] section  [F2] apply  [o] editor  [F5] reload"
            }
            Focus::Modules => {
                "[↑↓] row  [Enter] add  [x] remove  [Tab] section  [F2] apply  [o] editor  [F5] reload"
            }
            Focus::Aliases => {
                "[↑↓] row  [Enter] add  [e] edit  [x] remove  [Tab] section  [F2] apply  [o] editor"
            }
            Focus::Custom => {
                "[Space/Enter] toggle  [Tab] section  [F2] apply  [o] editor  [F5] reload  [q] quit"
            }
        };
        f.render_widget(
            Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
            area,
        );
    }

    fn render_confirm_modal(&self, f: &mut Frame, area: Rect) {
        let modal = centered(area, 60, 30);
        f.render_widget(Clear, modal);
        let text = format!(
            "Apply pending profile changes to\n{}?\n\n[y] confirm    any other key cancels",
            self.profile_path.display()
        );
        f.render_widget(
            Paragraph::new(text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Confirm apply "),
                )
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: false }),
            modal,
        );
    }
}

fn render_prompt(f: &mut Frame, area: Rect, prompt: &Prompt) {
    let modal = centered(area, 60, 25);
    f.render_widget(Clear, modal);
    let lines = vec![
        Line::from(Span::styled(
            prompt.hint.clone(),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(format!("> {}_", prompt.buffer)),
        Line::from(""),
        Line::from(Span::styled(
            "[Enter] confirm    [Esc] cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", prompt.title)),
            )
            .wrap(Wrap { trim: false }),
        modal,
    );
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
    f.render_widget(
        Paragraph::new(msg)
            .block(Block::default().borders(Borders::ALL).title(" info "))
            .alignment(Alignment::Center),
        rect,
    );
}

fn ensure_file_exists(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir {}", parent.display()))?;
    }
    std::fs::write(path, "").with_context(|| format!("creating empty file {}", path.display()))?;
    Ok(())
}

fn launch_editor(path: &Path) -> Result<()> {
    launch_external(&path.to_string_lossy())
}

#[cfg(windows)]
fn launch_external(target: &str) -> Result<()> {
    Command::new("cmd")
        .args(["/C", "start", "", target])
        .spawn()
        .with_context(|| format!("spawning cmd start for {target}"))?;
    Ok(())
}

#[cfg(not(windows))]
fn launch_external(target: &str) -> Result<()> {
    Command::new("xdg-open")
        .arg(target)
        .spawn()
        .with_context(|| format!("spawning xdg-open for {target}"))?;
    Ok(())
}

fn check(b: bool) -> char {
    if b { 'x' } else { ' ' }
}

fn cycle_enum<T: Copy + PartialEq>(all: &[T], current: T, direction: i32) -> T {
    if all.is_empty() {
        return current;
    }
    let idx = all.iter().position(|v| *v == current).unwrap_or(0) as i32;
    let len = all.len() as i32;
    let next = (idx + direction).rem_euclid(len) as usize;
    all[next]
}

fn section_header(title: &str, focused: bool) -> Line<'static> {
    let marker = if focused { "▶ " } else { "  " };
    let style = if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    Line::from(Span::styled(format!("{marker}{title}"), style))
}

fn row_line(text: &str, selected: bool) -> Line<'static> {
    let prefix = if selected { "  > " } else { "    " };
    let style = if selected {
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Line::from(Span::styled(format!("{prefix}{text}"), style))
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
    use crossterm::event::{KeyEvent, KeyModifiers};
    use std::fs;
    use tempfile::TempDir;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn screen_with_content(content: &str) -> (TempDir, ProfileScreen) {
        let td = TempDir::new().unwrap();
        let path = td.path().join("profile.ps1");
        if !content.is_empty() {
            fs::write(&path, content).unwrap();
        }
        let screen = ProfileScreen::new(path);
        (td, screen)
    }

    #[test]
    fn new_with_missing_file_starts_with_defaults() {
        let (_td, screen) = screen_with_content("");
        assert!(!screen.file_present);
        assert_eq!(screen.settings, Settings::default());
        assert!(!screen.is_dirty());
    }

    #[test]
    fn new_parses_existing_managed_block() {
        let content = format!(
            "$x = 1\n{}\nSet-PSReadLineOption -HistoryNoDuplicates:$true\nImport-Module posh-git\n{}\n",
            block::START_MARKER,
            block::END_MARKER
        );
        let (_td, screen) = screen_with_content(&content);
        assert!(screen.settings.psreadline.history_no_duplicates);
        assert_eq!(screen.settings.modules, vec!["posh-git"]);
        assert!(!screen.is_dirty());
    }

    #[test]
    fn corrupted_block_disables_editing() {
        let content = format!(
            "{}\n{}\nx\n{}\n",
            block::START_MARKER,
            block::START_MARKER,
            block::END_MARKER
        );
        let (_td, mut screen) = screen_with_content(&content);
        assert!(screen.corrupted_error.is_some());
        screen.handle_key(key(KeyCode::Char(' ')));
        assert!(!screen.settings.psreadline.history_no_duplicates);
    }

    #[test]
    fn space_toggles_bool_option() {
        let (_td, mut screen) = screen_with_content("");
        screen.psreadline_row = 0;
        screen.handle_key(key(KeyCode::Char(' ')));
        assert!(screen.settings.psreadline.history_no_duplicates);
        assert!(screen.is_dirty());
        screen.handle_key(key(KeyCode::Char(' ')));
        assert!(!screen.settings.psreadline.history_no_duplicates);
    }

    #[test]
    fn right_cycles_enum_forward() {
        let (_td, mut screen) = screen_with_content("");
        screen.psreadline_row = 2;
        assert_eq!(
            screen.settings.psreadline.prediction_source,
            PredictionSource::None
        );
        screen.handle_key(key(KeyCode::Right));
        assert_eq!(
            screen.settings.psreadline.prediction_source,
            PredictionSource::History
        );
    }

    #[test]
    fn left_cycles_enum_backward() {
        let (_td, mut screen) = screen_with_content("");
        screen.psreadline_row = 2;
        screen.handle_key(key(KeyCode::Left));
        assert_eq!(
            screen.settings.psreadline.prediction_source,
            PredictionSource::HistoryAndPlugin
        );
    }

    #[test]
    fn tab_cycles_focus() {
        let (_td, mut screen) = screen_with_content("");
        assert_eq!(screen.focus, Focus::PSReadLine);
        screen.handle_key(key(KeyCode::Tab));
        assert_eq!(screen.focus, Focus::Modules);
        screen.handle_key(key(KeyCode::Tab));
        assert_eq!(screen.focus, Focus::Aliases);
        screen.handle_key(key(KeyCode::Tab));
        assert_eq!(screen.focus, Focus::Custom);
        screen.handle_key(key(KeyCode::Tab));
        assert_eq!(screen.focus, Focus::PSReadLine);
    }

    #[test]
    fn add_module_flow() {
        let (_td, mut screen) = screen_with_content("");
        screen.focus = Focus::Modules;
        screen.modules_row = 0;
        screen.handle_key(key(KeyCode::Enter));
        assert!(screen.prompt.is_some());
        for c in "posh-git".chars() {
            screen.handle_key(key(KeyCode::Char(c)));
        }
        screen.handle_key(key(KeyCode::Enter));
        assert_eq!(screen.settings.modules, vec!["posh-git"]);
    }

    #[test]
    fn add_module_rejects_duplicate() {
        let content = format!(
            "{}\nImport-Module posh-git\n{}\n",
            block::START_MARKER,
            block::END_MARKER
        );
        let (_td, mut screen) = screen_with_content(&content);
        screen.focus = Focus::Modules;
        screen.modules_row = screen.settings.modules.len();
        screen.handle_key(key(KeyCode::Enter));
        for c in "posh-git".chars() {
            screen.handle_key(key(KeyCode::Char(c)));
        }
        screen.handle_key(key(KeyCode::Enter));
        assert_eq!(screen.settings.modules.len(), 1);
    }

    #[test]
    fn remove_module() {
        let content = format!(
            "{}\nImport-Module posh-git\nImport-Module Terminal-Icons\n{}\n",
            block::START_MARKER,
            block::END_MARKER
        );
        let (_td, mut screen) = screen_with_content(&content);
        screen.focus = Focus::Modules;
        screen.modules_row = 0;
        screen.handle_key(key(KeyCode::Char('x')));
        assert_eq!(screen.settings.modules, vec!["Terminal-Icons"]);
    }

    #[test]
    fn add_alias_two_step_flow() {
        let (_td, mut screen) = screen_with_content("");
        screen.focus = Focus::Aliases;
        screen.aliases_row = 0;
        screen.handle_key(key(KeyCode::Enter));
        assert!(matches!(
            screen.prompt.as_ref().map(|p| &p.kind),
            Some(PromptKind::AddAliasName)
        ));
        for c in "ll".chars() {
            screen.handle_key(key(KeyCode::Char(c)));
        }
        screen.handle_key(key(KeyCode::Enter));
        assert!(matches!(
            screen.prompt.as_ref().map(|p| &p.kind),
            Some(PromptKind::AddAliasValue { .. })
        ));
        for c in "Get-ChildItem".chars() {
            screen.handle_key(key(KeyCode::Char(c)));
        }
        screen.handle_key(key(KeyCode::Enter));
        assert_eq!(
            screen.settings.aliases.get("ll").map(String::as_str),
            Some("Get-ChildItem")
        );
    }

    #[test]
    fn edit_alias_value() {
        let content = format!(
            "{}\nSet-Alias ll 'Get-ChildItem'\n{}\n",
            block::START_MARKER,
            block::END_MARKER
        );
        let (_td, mut screen) = screen_with_content(&content);
        screen.focus = Focus::Aliases;
        screen.aliases_row = 0;
        screen.handle_key(key(KeyCode::Char('e')));
        assert!(screen.prompt.is_some());
        screen.handle_key(key(KeyCode::Backspace));
        screen.handle_key(key(KeyCode::Char('X')));
        screen.handle_key(key(KeyCode::Enter));
        assert_eq!(
            screen.settings.aliases.get("ll").map(String::as_str),
            Some("Get-ChildIteX")
        );
    }

    #[test]
    fn remove_alias() {
        let content = format!(
            "{}\nSet-Alias ll 'Get-ChildItem'\n{}\n",
            block::START_MARKER,
            block::END_MARKER
        );
        let (_td, mut screen) = screen_with_content(&content);
        screen.focus = Focus::Aliases;
        screen.aliases_row = 0;
        screen.handle_key(key(KeyCode::Char('x')));
        assert!(screen.settings.aliases.is_empty());
    }

    #[test]
    fn custom_section_toggles() {
        let (_td, mut screen) = screen_with_content("");
        screen.focus = Focus::Custom;
        assert!(!screen.custom_expanded);
        screen.handle_key(key(KeyCode::Enter));
        assert!(screen.custom_expanded);
    }

    #[test]
    fn escape_cancels_prompt() {
        let (_td, mut screen) = screen_with_content("");
        screen.focus = Focus::Modules;
        screen.handle_key(key(KeyCode::Enter));
        assert!(screen.prompt.is_some());
        screen.handle_key(key(KeyCode::Esc));
        assert!(screen.prompt.is_none());
    }

    #[test]
    fn apply_rejects_when_clean() {
        let (_td, mut screen) = screen_with_content("");
        screen.handle_key(key(KeyCode::F(2)));
        assert!(!screen.confirm_apply);
    }

    #[test]
    fn apply_prompts_when_dirty() {
        let (_td, mut screen) = screen_with_content("");
        screen.handle_key(key(KeyCode::Char(' ')));
        screen.handle_key(key(KeyCode::F(2)));
        assert!(screen.confirm_apply);
    }

    #[test]
    fn apply_writes_block_and_backup() {
        let td = TempDir::new().unwrap();
        let profile_path = td.path().join("profile.ps1");
        fs::write(&profile_path, "$x = 1\n").unwrap();
        let mut screen = ProfileScreen::new(profile_path.clone());
        screen.settings.psreadline.history_no_duplicates = true;
        screen.settings.modules.push("posh-git".to_string());
        assert!(screen.is_dirty());

        // Route backups to a temp dir so do_apply doesn't touch the user's real dir.
        unsafe {
            std::env::set_var("HOME", td.path());
            std::env::set_var("USERPROFILE", td.path());
        }

        let ts = screen.do_apply().expect("apply");
        let written = fs::read_to_string(&profile_path).unwrap();
        assert!(written.starts_with("$x = 1\n"));
        assert!(written.contains(block::START_MARKER));
        assert!(written.contains("HistoryNoDuplicates:$true"));
        assert!(written.contains("Import-Module posh-git"));
        assert!(!ts.is_empty());
    }

    #[test]
    fn quit_returns_quit_action() {
        let (_td, mut screen) = screen_with_content("");
        let action = screen.handle_key(key(KeyCode::Char('q')));
        assert_eq!(action, ScreenAction::Quit);
    }
}
