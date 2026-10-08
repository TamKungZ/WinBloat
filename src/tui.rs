use crate::cli::Args;
use crate::format::{format_timestamp, human_size, size_tier};
use crate::scanner;
use crate::tree::{SortKey, Tree, NONE};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const APP_NAME: &str = "WinBloat";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHOR_NAME: &str = "TamKungZ_";
const AUTHOR_EMAIL: &str = "dev@tamkungz.me";

pub fn run(_args: &Args, root: &Path) -> io::Result<()> {
    let start = Instant::now();
    let tree = scanner::scan(root)?;
    let elapsed = start.elapsed();

    let mut app = App::new(tree, root.to_path_buf(), elapsed);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| draw(f, app))?;
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if app.handle_key(key.code) {
                return Ok(());
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortMode {
    Size,
    Name,
    Recent,
    Modified,
}

impl SortMode {
    fn key(self) -> SortKey {
        match self {
            SortMode::Size => SortKey::Size,
            SortMode::Name => SortKey::Name,
            SortMode::Recent => SortKey::Recent,
            SortMode::Modified => SortKey::Modified,
        }
    }

    fn label(self) -> &'static str {
        match self {
            SortMode::Size => "size",
            SortMode::Name => "name",
            SortMode::Recent => "recently accessed",
            SortMode::Modified => "recently modified",
        }
    }

    fn next(self) -> Self {
        match self {
            SortMode::Size => SortMode::Name,
            SortMode::Name => SortMode::Modified,
            SortMode::Modified => SortMode::Recent,
            SortMode::Recent => SortMode::Size,
        }
    }
}

struct App {
    tree: Tree,
    root: PathBuf,
    elapsed: Duration,
    expanded: HashSet<u32>,
    visible: Vec<(u32, usize)>,
    list_state: ListState,
    sort: SortMode,
}

impl App {
    fn new(tree: Tree, root: PathBuf, elapsed: Duration) -> Self {
        let mut app = Self {
            tree,
            root,
            elapsed,
            expanded: HashSet::new(),
            visible: Vec::new(),
            list_state: ListState::default(),
            sort: SortMode::Size,
        };
        app.expanded.insert(0);
        app.rebuild_visible();
        app.list_state.select(Some(0));
        app
    }

    fn rebuild_visible(&mut self) {
        let mut out: Vec<(u32, usize)> = Vec::with_capacity(self.tree.nodes.len().min(4096));
        Self::collect(&self.tree, &self.expanded, self.sort, 0, 0, &mut out);
        self.visible = out;
        if let Some(sel) = self.list_state.selected() {
            if sel >= self.visible.len() && !self.visible.is_empty() {
                self.list_state.select(Some(self.visible.len() - 1));
            }
        }
    }

    fn collect(
        tree: &Tree,
        expanded: &HashSet<u32>,
        sort: SortMode,
        id: u32,
        depth: usize,
        out: &mut Vec<(u32, usize)>,
    ) {
        out.push((id, depth));
        if !expanded.contains(&id) {
            return;
        }
        for c in tree.sorted_children(id, sort.key()) {
            Self::collect(tree, expanded, sort, c, depth + 1, out);
        }
    }

    fn selected_id(&self) -> Option<u32> {
        self.list_state
            .selected()
            .and_then(|i| self.visible.get(i).map(|&(id, _)| id))
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::Home | KeyCode::Char('g') => self.list_state.select(Some(0)),
            KeyCode::End | KeyCode::Char('G') => {
                if !self.visible.is_empty() {
                    self.list_state.select(Some(self.visible.len() - 1));
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.expand_selected(),
            KeyCode::Left | KeyCode::Char('h') => self.collapse_selected(),
            KeyCode::Char(' ') => self.toggle_selected(),
            KeyCode::Char('s') => {
                self.sort = self.sort.next();
                self.rebuild_visible();
            }
            _ => {}
        }
        false
    }

    fn move_selection(&mut self, delta: isize) {
        if self.visible.is_empty() {
            return;
        }
        let len = self.visible.len() as isize;
        let cur = self.list_state.selected().unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, len - 1);
        self.list_state.select(Some(next as usize));
    }

    fn expand_selected(&mut self) {
        if let Some(id) = self.selected_id() {
            if self.tree.nodes[id as usize].is_dir {
                self.expanded.insert(id);
                self.rebuild_visible();
            }
        }
    }

    fn collapse_selected(&mut self) {
        if let Some(id) = self.selected_id() {
            if id != 0 && self.expanded.contains(&id) {
                self.expanded.remove(&id);
                self.rebuild_visible();
            } else if id != 0 {
                let parent = self.tree.nodes[id as usize].parent;
                if parent != NONE {
                    if let Some(pos) = self.visible.iter().position(|&(i, _)| i == parent) {
                        self.list_state.select(Some(pos));
                    }
                }
            }
        }
    }

    fn toggle_selected(&mut self) {
        if let Some(id) = self.selected_id() {
            if !self.tree.nodes[id as usize].is_dir {
                return;
            }
            if self.expanded.contains(&id) {
                self.expanded.remove(&id);
            } else {
                self.expanded.insert(id);
            }
            self.rebuild_visible();
        }
    }
}

fn color_for(size: u64) -> Color {
    match size_tier(size) {
        3 => Color::Red,
        2 => Color::Yellow,
        1 => Color::Green,
        _ => Color::Cyan,
    }
}

fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(area);

    draw_header(f, app, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(chunks[1]);

    draw_tree(f, app, body[0]);
    draw_details(f, app, body[1]);
    draw_footer(f, app, chunks[2]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let total = app.tree.nodes[0].size;
    let lines = vec![
        Line::from(vec![
            Span::styled(
                APP_NAME,
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("v{}", APP_VERSION),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("   "),
            Span::styled(
                format!("by {} <{}>", AUTHOR_NAME, AUTHOR_EMAIL),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled("Path:  ", Style::default().fg(Color::DarkGray)),
            Span::raw(app.root.display().to_string()),
        ]),
        Line::from(vec![
            Span::styled("Size:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(human_size(total), Style::default().fg(Color::Green)),
            Span::raw("   "),
            Span::styled("Items: ", Style::default().fg(Color::DarkGray)),
            Span::raw(app.tree.nodes.len().to_string()),
            Span::raw("   "),
            Span::styled("Time:  ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{:.2?}", app.elapsed)),
            Span::raw("   "),
            Span::styled("Sort:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(app.sort.label(), Style::default().fg(Color::Yellow)),
        ]),
    ];

    let p = Paragraph::new(lines).block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(p, area);
}

fn draw_tree(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .visible
        .iter()
        .map(|&(id, depth)| {
            let node = &app.tree.nodes[id as usize];
            let indent = "  ".repeat(depth);
            let marker = if node.is_dir {
                if app.expanded.contains(&id) {
                    "v "
                } else {
                    "> "
                }
            } else {
                "  "
            };
            let name_style = if node.is_dir {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::raw(indent),
                Span::styled(marker, Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:>10}  ", human_size(node.size)),
                    Style::default().fg(color_for(node.size)),
                ),
                Span::styled(app.tree.name(id).to_string(), name_style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Tree "))
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(38, 38, 38))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_details(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Details ");
    let Some(id) = app.selected_id() else {
        f.render_widget(block, area);
        return;
    };

    let node = &app.tree.nodes[id as usize];
    let total = app.tree.nodes[0].size.max(1);
    let percent_total = node.size as f64 / total as f64 * 100.0;
    let parent_pct = if node.parent != NONE {
        let psize = app.tree.nodes[node.parent as usize].size.max(1);
        node.size as f64 / psize as f64 * 100.0
    } else {
        100.0
    };

    let mut lines = vec![
        kv("Name", app.tree.name(id).to_string()),
        Line::from(""),
        kv_colored("Size", human_size(node.size), color_for(node.size)),
        kv(
            "Share",
            format!("{:.2}% of parent   {:.2}% of total", parent_pct, percent_total),
        ),
        kv(
            "Type",
            if node.is_dir { "Directory".into() } else { "File".into() },
        ),
    ];

    if node.is_dir {
        lines.push(kv("Children", app.tree.child_count(id).to_string()));
    }

    lines.push(Line::from(""));
    lines.push(kv("Modified", format_timestamp(node.modified)));
    lines.push(kv("Accessed", format_timestamp(node.accessed)));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "     Path",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(app.tree.full_path(id)));

    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn kv(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:>10}  ", label), Style::default().fg(Color::DarkGray)),
        Span::raw(value),
    ])
}

fn kv_colored(label: &str, value: String, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:>10}  ", label), Style::default().fg(Color::DarkGray)),
        Span::styled(value, Style::default().fg(color).add_modifier(Modifier::BOLD)),
    ])
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let selected = app.list_state.selected().map(|i| i + 1).unwrap_or(0);
    let total = app.visible.len();
    let keys = "  q Quit   j/k Move   h/l Collapse/Expand   space Toggle   s Sort   g/G Top/Bottom";
    let counter = format!("  {} / {}", selected, total);

    let lines = vec![
        Line::from(Span::styled(keys, Style::default().fg(Color::DarkGray))),
        Line::from(Span::styled(counter, Style::default().fg(Color::DarkGray))),
    ];
    f.render_widget(Paragraph::new(lines), area);
}