mod systemd;

use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Terminal;

const GREEN: Color = Color::Rgb(0, 255, 200);
const DIM_GREEN: Color = Color::Rgb(0, 128, 100);
const RED: Color = Color::Rgb(255, 60, 60);
const YELLOW: Color = Color::Rgb(255, 255, 0);

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Services,
    Boot,
    Journal,
}

struct App {
    tab: Tab,
    services: Vec<systemd::ServiceInfo>,
    boot_time: Option<systemd::BootTime>,
    blame: Vec<systemd::BlameEntry>,
    journal: Vec<systemd::JournalEntry>,
    sys_info: systemd::SystemInfo,
    scroll: usize,
    filter: String,
    filter_active: bool,
}

impl App {
    fn new() -> Self {
        Self {
            tab: Tab::Services,
            services: systemd::list_services(),
            boot_time: systemd::get_boot_time(),
            blame: systemd::get_blame(30),
            journal: systemd::get_journal(200, None, None),
            sys_info: systemd::get_system_info(),
            scroll: 0,
            filter: String::new(),
            filter_active: false,
        }
    }

    fn refresh(&mut self) {
        match self.tab {
            Tab::Services => self.services = systemd::list_services(),
            Tab::Boot => {
                self.boot_time = systemd::get_boot_time();
                self.blame = systemd::get_blame(30);
            }
            Tab::Journal => self.journal = systemd::get_journal(200, None, None),
        }
    }

    fn filtered_services(&self) -> Vec<&systemd::ServiceInfo> {
        if self.filter.is_empty() {
            self.services.iter().collect()
        } else {
            let f = self.filter.to_lowercase();
            self.services.iter().filter(|s| {
                s.name.to_lowercase().contains(&f) || s.description.to_lowercase().contains(&f)
            }).collect()
        }
    }

    fn max_scroll(&self) -> usize {
        match self.tab {
            Tab::Services => self.filtered_services().len().saturating_sub(1),
            Tab::Boot => self.blame.len().saturating_sub(1),
            Tab::Journal => self.journal.len().saturating_sub(1),
        }
    }
}

fn main() -> io::Result<()> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let result = run(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    let tick = Duration::from_secs(5);
    let mut last_refresh = Instant::now();

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(2),
                    Constraint::Min(8),
                    Constraint::Length(1),
                ])
                .split(f.area());

            draw_header(f, chunks[0], app);
            draw_tabs(f, chunks[1], app);
            match app.tab {
                Tab::Services => draw_services(f, chunks[2], app),
                Tab::Boot => draw_boot(f, chunks[2], app),
                Tab::Journal => draw_journal(f, chunks[2], app),
            }
            draw_footer(f, chunks[3], app);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if app.filter_active {
                    match key.code {
                        KeyCode::Esc => {
                            app.filter_active = false;
                            app.filter.clear();
                        }
                        KeyCode::Enter => app.filter_active = false,
                        KeyCode::Char(c) => app.filter.push(c),
                        KeyCode::Backspace => { app.filter.pop(); }
                        _ => {}
                    }
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(()),
                    KeyCode::Char('1') => { app.tab = Tab::Services; app.scroll = 0; }
                    KeyCode::Char('2') => { app.tab = Tab::Boot; app.scroll = 0; }
                    KeyCode::Char('3') => { app.tab = Tab::Journal; app.scroll = 0; }
                    KeyCode::Tab => {
                        app.tab = match app.tab {
                            Tab::Services => Tab::Boot,
                            Tab::Boot => Tab::Journal,
                            Tab::Journal => Tab::Services,
                        };
                        app.scroll = 0;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        if app.scroll < app.max_scroll() { app.scroll += 1; }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        app.scroll = app.scroll.saturating_sub(1);
                    }
                    KeyCode::Char('G') => app.scroll = app.max_scroll(),
                    KeyCode::Char('g') => app.scroll = 0,
                    KeyCode::Char('r') => app.refresh(),
                    KeyCode::Char('/') => {
                        app.filter_active = true;
                        app.filter.clear();
                    }
                    _ => {}
                }
            }
        }

        if last_refresh.elapsed() >= tick {
            app.refresh();
            last_refresh = Instant::now();
        }
    }
}

fn draw_header(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let info = &app.sys_info;
    let title = format!(
        " {} │ kernel {} │ {} │ {} ",
        info.hostname, info.kernel, info.uptime, info.systemd_version
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(GREEN))
        .title(Span::styled("boot-dash", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)));
    let para = Paragraph::new(Line::from(Span::styled(title, Style::default().fg(GREEN))))
        .block(block);
    f.render_widget(para, area);
}

fn draw_tabs(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let tabs = vec![
        (" 1:Services ", app.tab == Tab::Services),
        (" 2:Boot ", app.tab == Tab::Boot),
        (" 3:Journal ", app.tab == Tab::Journal),
    ];
    let spans: Vec<Span> = tabs.iter().map(|(label, active)| {
        if *active {
            Span::styled(*label, Style::default().fg(Color::Black).bg(GREEN).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(*label, Style::default().fg(DIM_GREEN))
        }
    }).collect();
    let line = Line::from(spans);
    f.render_widget(Paragraph::new(line), area);
}

fn status_color(state: &str) -> Color {
    match state {
        "running" | "listening" | "mounted" => GREEN,
        "exited" | "inactive" | "waiting" => DIM_GREEN,
        "failed" => RED,
        "activating" | "deactivating" => YELLOW,
        _ => DIM_GREEN,
    }
}

fn draw_services(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let filtered = app.filtered_services();
    let header = Row::new(vec![
        Cell::from("Unit").style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
        Cell::from("State").style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
        Cell::from("Sub").style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
        Cell::from("Description").style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
    ]);

    let visible_height = area.height.saturating_sub(3) as usize;
    let start = app.scroll;
    let end = (start + visible_height).min(filtered.len());

    let rows: Vec<Row> = filtered[start..end].iter().enumerate().map(|(i, s)| {
        let color = status_color(s.status_display());
        let highlight = if i + start == app.scroll {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };
        Row::new(vec![
            Cell::from(s.name.clone()).style(highlight),
            Cell::from(s.status_display()).style(Style::default().fg(color)),
            Cell::from(s.sub_state.clone()).style(Style::default().fg(DIM_GREEN)),
            Cell::from(s.description.clone()).style(Style::default().fg(DIM_GREEN)),
        ])
    }).collect();

    let title = format!("Services ({}/{})", filtered.len(), app.services.len());
    let table = Table::new(
        rows,
        [
            Constraint::Length(35),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(DIM_GREEN))
            .title(Span::styled(title, Style::default().fg(GREEN))),
    );
    f.render_widget(table, area);
}

fn draw_boot(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(5)])
        .split(area);

    // Boot time summary
    let boot_text = if let Some(ref bt) = app.boot_time {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("  Total: ", Style::default().fg(DIM_GREEN)),
                Span::styled(&bt.total, Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  Kernel: ", Style::default().fg(DIM_GREEN)),
                Span::styled(&bt.kernel, Style::default().fg(GREEN)),
                Span::styled("  Userspace: ", Style::default().fg(DIM_GREEN)),
                Span::styled(&bt.userspace, Style::default().fg(GREEN)),
            ]),
        ];
        if let Some(ref fw) = bt.firmware {
            lines.push(Line::from(vec![
                Span::styled("  Firmware: ", Style::default().fg(DIM_GREEN)),
                Span::styled(fw, Style::default().fg(GREEN)),
            ]));
        }
        lines
    } else {
        vec![Line::from(Span::styled("  (no boot data)", Style::default().fg(RED)))]
    };

    let boot_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIM_GREEN))
        .title(Span::styled("Boot Time", Style::default().fg(GREEN)));
    f.render_widget(Paragraph::new(boot_text).block(boot_block), chunks[0]);

    // Blame
    let header = Row::new(vec![
        Cell::from("Time").style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
        Cell::from("Unit").style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
    ]);

    let visible = chunks[1].height.saturating_sub(3) as usize;
    let start = app.scroll;
    let end = (start + visible).min(app.blame.len());

    let rows: Vec<Row> = app.blame[start..end].iter().map(|b| {
        let color = if b.time_ms > 5000 { RED }
                    else if b.time_ms > 1000 { YELLOW }
                    else { GREEN };
        Row::new(vec![
            Cell::from(b.time_str.clone()).style(Style::default().fg(color)),
            Cell::from(b.unit.clone()).style(Style::default().fg(DIM_GREEN)),
        ])
    }).collect();

    let blame_table = Table::new(
        rows,
        [Constraint::Length(15), Constraint::Min(30)],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(DIM_GREEN))
            .title(Span::styled(
                format!("Blame (top {})", app.blame.len()),
                Style::default().fg(GREEN),
            )),
    );
    f.render_widget(blame_table, chunks[1]);
}

fn draw_journal(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let header = Row::new(vec![
        Cell::from("Time").style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
        Cell::from("Unit").style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
        Cell::from("Message").style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
    ]);

    let visible = area.height.saturating_sub(3) as usize;
    let start = app.scroll;
    let end = (start + visible).min(app.journal.len());

    let rows: Vec<Row> = app.journal[start..end].iter().map(|j| {
        let color = match j.priority {
            0..=2 => RED,
            3 => RED,
            4 => YELLOW,
            5..=6 => GREEN,
            _ => DIM_GREEN,
        };
        Row::new(vec![
            Cell::from(j.timestamp.clone()).style(Style::default().fg(DIM_GREEN)),
            Cell::from(j.unit.clone()).style(Style::default().fg(GREEN)),
            Cell::from(j.message.clone()).style(Style::default().fg(color)),
        ])
    }).collect();

    let table = Table::new(
        rows,
        [Constraint::Length(16), Constraint::Length(25), Constraint::Min(30)],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(DIM_GREEN))
            .title(Span::styled(
                format!("Journal ({} entries)", app.journal.len()),
                Style::default().fg(GREEN),
            )),
    );
    f.render_widget(table, area);
}

fn draw_footer(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let spans = if app.filter_active {
        vec![
            Span::styled(" /", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(&app.filter, Style::default().fg(GREEN)),
            Span::styled("█", Style::default().fg(GREEN)),
        ]
    } else {
        vec![
            Span::styled(" q", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" quit  ", Style::default().fg(DIM_GREEN)),
            Span::styled("Tab", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" switch  ", Style::default().fg(DIM_GREEN)),
            Span::styled("j/k", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" scroll  ", Style::default().fg(DIM_GREEN)),
            Span::styled("r", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" refresh  ", Style::default().fg(DIM_GREEN)),
            Span::styled("/", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" filter", Style::default().fg(DIM_GREEN)),
        ]
    };
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
