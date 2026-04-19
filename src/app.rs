use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs, Wrap},
};
use std::io::{self, Stdout};
use std::time::Duration;

use crate::history::{
    dedup::find_duplicates,
    parse::parse,
    typos::{DEFAULT_DENYLIST, find_typos},
};
use crate::inventory::Inventory;
use crate::paths;
use crate::ui::history::{HistoryScreen, ScreenAction as HistoryAction};
use crate::ui::profile::{ProfileScreen, ScreenAction as ProfileAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    History,
    Profile,
    About,
}

impl Tab {
    const ORDER: [Tab; 3] = [Tab::History, Tab::Profile, Tab::About];

    fn index(self) -> usize {
        match self {
            Tab::History => 0,
            Tab::Profile => 1,
            Tab::About => 2,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tab::History => "History",
            Tab::Profile => "Profile",
            Tab::About => "About",
        }
    }

    fn next(self) -> Self {
        match self {
            Tab::History => Tab::Profile,
            Tab::Profile => Tab::About,
            Tab::About => Tab::History,
        }
    }

    fn prev(self) -> Self {
        match self {
            Tab::History => Tab::About,
            Tab::Profile => Tab::History,
            Tab::About => Tab::Profile,
        }
    }
}

struct App {
    active_tab: Tab,
    history: HistoryScreen,
    profile: ProfileScreen,
    quit_confirm: bool,
}

impl App {
    fn is_dirty(&self) -> bool {
        self.history.pending_count() > 0 || self.profile.is_dirty()
    }
}

pub fn run() -> Result<()> {
    let app = build_app()?;
    let mut terminal = setup_terminal()?;
    let result = run_loop(&mut terminal, app);
    restore_terminal(&mut terminal)?;
    result
}

fn build_app() -> Result<App> {
    let history_path = paths::history_file()?;
    let (content, present) = match std::fs::read_to_string(&history_path) {
        Ok(c) => (c, true),
        Err(_) => (String::new(), false),
    };
    let entries = parse(&content);
    let duplicate_groups = find_duplicates(&entries);
    let inventory = Inventory::load_or_embedded(&paths::inventory_cache()?)?;
    let typo_flags = find_typos(&entries, &inventory, DEFAULT_DENYLIST);
    let history = HistoryScreen::new(
        history_path,
        inventory,
        present,
        entries,
        duplicate_groups,
        typo_flags,
    );

    let profile_path = paths::profile_file()?;
    let profile = ProfileScreen::new(profile_path);

    Ok(App {
        active_tab: Tab::History,
        history,
        profile,
        quit_confirm: false,
    })
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, mut app: App) -> Result<()> {
    loop {
        terminal.draw(|f| render(f, &app))?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if app.quit_confirm {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(()),
                    _ => app.quit_confirm = false,
                }
                continue;
            }

            match key.code {
                KeyCode::F(3) => {
                    app.active_tab = app.active_tab.next();
                    continue;
                }
                KeyCode::BackTab => {
                    app.active_tab = app.active_tab.prev();
                    continue;
                }
                _ => {}
            }

            let wants_quit = match app.active_tab {
                Tab::History => {
                    matches!(app.history.handle_key(key), HistoryAction::Quit)
                }
                Tab::Profile => {
                    matches!(app.profile.handle_key(key), ProfileAction::Quit)
                }
                Tab::About => matches!(key.code, KeyCode::Char('q') | KeyCode::F(10)),
            };
            if wants_quit {
                if app.is_dirty() {
                    app.quit_confirm = true;
                } else {
                    return Ok(());
                }
            }
        }
        app.history.tick();
        app.profile.tick();
    }
}

fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_tabs(f, chunks[0], app);
    match app.active_tab {
        Tab::History => app.history.render(f, chunks[1]),
        Tab::Profile => app.profile.render(f, chunks[1]),
        Tab::About => render_about(f, chunks[1]),
    }
    render_status_bar(f, chunks[2], app);

    if app.quit_confirm {
        render_quit_confirm(f, f.area());
    }
}

fn render_tabs(f: &mut Frame, area: Rect, app: &App) {
    let titles: Vec<Line> = Tab::ORDER
        .iter()
        .map(|t| Line::from(format!(" {} ", t.label())))
        .collect();
    let dirty_suffix = if app.is_dirty() { "dirty" } else { "clean" };
    let tabs = Tabs::new(titles)
        .select(app.active_tab.index())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" powershellknife — {dirty_suffix} ")),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, area);
}

fn render_about(f: &mut Frame, area: Rect) {
    let version = env!("CARGO_PKG_VERSION");
    let history = paths::history_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("unresolved: {e}"));
    let profile = paths::profile_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("unresolved: {e}"));
    let data_dir = paths::app_data_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("unresolved: {e}"));
    let lines = vec![
        Line::from(Span::styled(
            format!("powershellknife v{version}"),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("history: {history}")),
        Line::from(format!("profile: {profile}")),
        Line::from(format!("data dir: {data_dir}")),
        Line::from(""),
        Line::from("Global keys: [F3] next tab  [Shift+Tab] prev tab  [q/F10] quit"),
        Line::from(
            "Backups live under ~/.powershellknife/backups/ — `psknife restore` to roll back.",
        ),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" About "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let tab_hint = "[F3] tab  [Shift+Tab] prev";
    let dirty_hint = if app.is_dirty() {
        "  unsaved changes — F2 to apply"
    } else {
        ""
    };
    let line = Line::from(format!("{tab_hint}{dirty_hint}"));
    f.render_widget(Paragraph::new(line).alignment(Alignment::Left), area);
}

fn render_quit_confirm(f: &mut Frame, area: Rect) {
    let modal = centered(area, 60, 25);
    f.render_widget(Clear, modal);
    let text = vec![
        Line::from(Span::styled(
            "Unsaved changes",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("You have pending changes that have not been applied."),
        Line::from("Quit anyway?"),
        Line::from(""),
        Line::from("[y] quit without applying    any other key cancels"),
    ];
    f.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" Quit "))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
        modal,
    );
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

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode().context("enabling raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("entering alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).context("creating terminal")
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().context("disabling raw mode")?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .context("leaving alternate screen")?;
    terminal.show_cursor().context("showing cursor")?;
    Ok(())
}
