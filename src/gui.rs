use crate::analysis::{self, FileTypeStats};
use crate::format::{format_timestamp, human_size, size_tier};
use crate::scanner;
use crate::tree::{NONE, Node, SortKey, Tree};
use eframe::egui::{
    self, Align, Color32, FontFamily, FontId, Layout, Rect, RichText, ScrollArea, Sense, Stroke,
    StrokeKind, TextStyle, Vec2,
};
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

const WINDOW_SIZE: [f32; 2] = [1440.0, 900.0];
const MAX_TREEMAP_CHILDREN: usize = 8;
const ACCENT: Color32 = Color32::from_rgb(0, 103, 192);

pub fn run(root: &Path) -> io::Result<()> {
    let mut app = WinBloatApp::new(empty_tree(root), root.to_path_buf(), Duration::ZERO);
    app.start_scan();
    let icon = load_app_icon()?;
    let windows_font = load_windows_font()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(WINDOW_SIZE)
            .with_min_inner_size([1120.0, 720.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "WinBloat",
        options,
        Box::new(|creation_context| {
            configure_style(&creation_context.egui_ctx, windows_font);
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| io::Error::other(error.to_string()))
}

fn empty_tree(root: &Path) -> Tree {
    let root_name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    let name_len = root_name.len() as u32;
    Tree {
        nodes: vec![Node {
            name_start: 0,
            name_len,
            parent: NONE,
            first_child: NONE,
            next_sibling: NONE,
            size: 0,
            modified: 0,
            accessed: 0,
            is_dir: true,
        }],
        names: root_name,
    }
}

struct WinBloatApp {
    tree: Tree,
    root: PathBuf,
    root_input: String,
    elapsed: Duration,
    expanded: HashSet<u32>,
    visible: Vec<(u32, usize)>,
    selected: u32,
    search: String,
    active_extension: Option<String>,
    sort: SortKey,
    file_types: Vec<FileTypeStats>,
    files: usize,
    directories: usize,
    index_memory: usize,
    treemap_focus: u32,
    treemap_children: Vec<u32>,
    scan_receiver: Option<Receiver<io::Result<ScanResult>>>,
    scan_in_progress: bool,
    status: Option<(bool, String)>,
}

struct ScanResult {
    tree: Tree,
    root: PathBuf,
    elapsed: Duration,
    file_types: Vec<FileTypeStats>,
    files: usize,
    directories: usize,
    index_memory: usize,
    treemap_children: Vec<u32>,
}

impl WinBloatApp {
    fn new(tree: Tree, root: PathBuf, elapsed: Duration) -> Self {
        let (files, directories) = analysis::item_counts(&tree);
        let index_memory = tree.nodes.len() * std::mem::size_of::<Node>() + tree.names.len();
        let file_types = analysis::file_type_stats(&tree);
        let treemap_children = analysis::largest_children(&tree, 0, MAX_TREEMAP_CHILDREN + 1);
        let mut app = Self {
            tree,
            root_input: display_root_path(&root),
            root,
            elapsed,
            expanded: HashSet::from([0]),
            visible: Vec::new(),
            selected: 0,
            search: String::new(),
            active_extension: None,
            sort: SortKey::Size,
            file_types,
            files,
            directories,
            index_memory,
            treemap_focus: 0,
            treemap_children,
            scan_receiver: None,
            scan_in_progress: false,
            status: None,
        };
        app.rebuild_visible();
        app
    }

    fn rebuild_visible(&mut self) {
        let mut visible = Vec::new();
        if self.search.is_empty() && self.active_extension.is_none() {
            Self::collect_visible(&self.tree, &self.expanded, self.sort, 0, 0, &mut visible);
        } else {
            let query = self.search.to_lowercase();
            let mut included = HashSet::new();
            for (index, node) in self.tree.nodes.iter().enumerate() {
                let id = index as u32;
                let name_matches =
                    query.is_empty() || self.tree.name(id).to_lowercase().contains(&query);
                let extension_matches = self.active_extension.as_ref().is_none_or(|extension| {
                    !node.is_dir && file_extension(self.tree.name(id)) == *extension
                });
                if name_matches && extension_matches {
                    let mut ancestor = id;
                    loop {
                        included.insert(ancestor);
                        let parent = self.tree.nodes[ancestor as usize].parent;
                        if parent == NONE {
                            break;
                        }
                        ancestor = parent;
                    }
                }
            }
            Self::collect_filtered(&self.tree, 0, 0, &included, self.sort, &mut visible);
        }
        self.visible = visible;
        if !self.visible.iter().any(|(id, _)| *id == self.selected) {
            self.selected = self.visible.first().map(|(id, _)| *id).unwrap_or(0);
        }
    }

    fn collect_visible(
        tree: &Tree,
        expanded: &HashSet<u32>,
        sort: SortKey,
        id: u32,
        depth: usize,
        out: &mut Vec<(u32, usize)>,
    ) {
        out.push((id, depth));
        if expanded.contains(&id) {
            for child in tree.sorted_children(id, sort) {
                Self::collect_visible(tree, expanded, sort, child, depth + 1, out);
            }
        }
    }

    fn collect_filtered(
        tree: &Tree,
        id: u32,
        depth: usize,
        included: &HashSet<u32>,
        sort: SortKey,
        out: &mut Vec<(u32, usize)>,
    ) {
        if !included.contains(&id) {
            return;
        }
        out.push((id, depth));
        for child in tree.sorted_children(id, sort) {
            Self::collect_filtered(tree, child, depth + 1, included, sort, out);
        }
    }

    fn start_scan(&mut self) {
        let requested_path = self.root_input.trim();
        if requested_path.is_empty() {
            self.status = Some((false, "Enter a folder path to scan.".to_string()));
            return;
        }
        let root = PathBuf::from(requested_path);
        let (sender, receiver) = mpsc::channel();
        self.scan_receiver = Some(receiver);
        self.scan_in_progress = true;
        self.status = Some((true, format!("Scanning {}…", root.display())));
        let worker = thread::Builder::new()
            .name("winbloat-scan".to_string())
            .spawn(move || {
                let start = Instant::now();
                let result = root.canonicalize().and_then(|root| {
                    if !root.is_dir() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("{} is not a directory", root.display()),
                        ));
                    }
                    scanner::scan(&root).map(|tree| {
                        let elapsed = start.elapsed();
                        let (files, directories) = analysis::item_counts(&tree);
                        let file_types = analysis::file_type_stats(&tree);
                        let index_memory =
                            tree.nodes.len() * std::mem::size_of::<Node>() + tree.names.len();
                        let treemap_children =
                            analysis::largest_children(&tree, 0, MAX_TREEMAP_CHILDREN + 1);
                        ScanResult {
                            tree,
                            root,
                            elapsed,
                            file_types,
                            files,
                            directories,
                            index_memory,
                            treemap_children,
                        }
                    })
                });
                let _ = sender.send(result);
            });
        if let Err(error) = worker {
            self.scan_receiver = None;
            self.scan_in_progress = false;
            self.status = Some((false, format!("Could not start scan: {error}")));
        }
    }

    fn poll_scan(&mut self) {
        let result = match self.scan_receiver.as_ref().map(Receiver::try_recv) {
            Some(Ok(result)) => Some(result),
            Some(Err(TryRecvError::Empty)) | None => None,
            Some(Err(TryRecvError::Disconnected)) => {
                self.scan_receiver = None;
                self.scan_in_progress = false;
                self.status = Some((false, "The scan worker stopped unexpectedly.".to_string()));
                return;
            }
        };
        let Some(result) = result else {
            return;
        };
        self.scan_receiver = None;
        self.scan_in_progress = false;
        match result {
            Ok(result) => {
                self.tree = result.tree;
                self.elapsed = result.elapsed;
                self.root = result.root.clone();
                self.root_input = display_root_path(&result.root);
                self.selected = 0;
                self.expanded.clear();
                self.expanded.insert(0);
                self.active_extension = None;
                self.treemap_focus = 0;
                self.file_types = result.file_types;
                self.files = result.files;
                self.directories = result.directories;
                self.index_memory = result.index_memory;
                self.treemap_children = result.treemap_children;
                self.status = Some((
                    true,
                    format!(
                        "Scan complete: {} files and {} folders in {:.2?}.",
                        self.files, self.directories, self.elapsed
                    ),
                ));
                self.rebuild_visible();
            }
            Err(error) => {
                self.status = Some((false, format!("Scan failed: {error}")));
            }
        }
    }

    fn scan_parent(&mut self) {
        if let Some(parent) = self.root.parent() {
            self.root_input = display_root_path(parent);
            self.start_scan();
        }
    }

    fn expand_all(&mut self) {
        self.expanded = self
            .tree
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.is_dir)
            .map(|(index, _)| index as u32)
            .collect();
        self.rebuild_visible();
    }

    fn copy_selected_path(&mut self, context: &egui::Context) {
        context.copy_text(self.item_path(self.selected).display().to_string());
        self.status = Some((true, "Path copied to clipboard.".to_string()));
    }

    fn open_selected(&mut self) {
        let node = self.tree.nodes[self.selected as usize];
        let path = self.item_path(self.selected);
        match reveal_in_explorer(&path, node.is_dir) {
            Ok(()) => self.status = Some((true, "Opened in Explorer.".to_string())),
            Err(error) => self.status = Some((false, format!("Could not open Explorer: {error}"))),
        }
    }

    fn draw_header(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(Color32::WHITE)
            .stroke(Stroke::new(1.0, Color32::from_rgb(215, 215, 215)))
            .inner_margin(egui::Margin::symmetric(14, 9))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("WinBloat").strong().size(21.0));
                    ui.separator();
                    ui.menu_button("File", |ui| {
                        if ui.button("Scan folder   Enter").clicked() {
                            self.start_scan();
                            ui.close();
                        }
                        if ui.button("Copy selected path").clicked() {
                            self.copy_selected_path(ui.ctx());
                            ui.close();
                        }
                        if ui.button("Show selected in Explorer").clicked() {
                            self.open_selected();
                            ui.close();
                        }
                    });
                    ui.menu_button("View", |ui| {
                        if ui.button("Expand all folders").clicked() {
                            self.expand_all();
                            ui.close();
                        }
                        if ui.button("Collapse all folders").clicked() {
                            self.expanded.clear();
                            self.expanded.insert(0);
                            self.rebuild_visible();
                            ui.close();
                        }
                        if ui.button("Clear filters").clicked() {
                            self.search.clear();
                            self.active_extension = None;
                            self.rebuild_visible();
                            ui.close();
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(if self.scan_in_progress {
                                "SCANNING"
                            } else {
                                "READ-ONLY"
                            })
                            .small()
                            .color(Color32::from_rgb(90, 105, 119)),
                        );
                    });
                });
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Scan location").color(Color32::from_rgb(75, 82, 90)));
                    let path_edit = ui.add(
                        egui::TextEdit::singleline(&mut self.root_input)
                            .hint_text(r"C:\Users\you\Downloads")
                            .desired_width((ui.available_width() - 225.0).max(200.0)),
                    );
                    #[cfg(windows)]
                    if ui.button("Browse…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_directory(&self.root)
                            .pick_folder()
                        {
                            self.root_input = display_root_path(&path);
                        }
                    }
                    let scan_clicked = ui
                        .add_enabled(
                            !self.scan_in_progress,
                            egui::Button::new(if self.scan_in_progress {
                                "Scanning…"
                            } else {
                                "Scan"
                            })
                            .fill(ACCENT),
                        )
                        .clicked();
                    if scan_clicked
                        || (path_edit.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter)))
                    {
                        self.start_scan();
                    }
                });
            });
        ui.add_space(3.0);
        let mut command = None;
        egui::Frame::new()
            .fill(Color32::from_rgb(250, 250, 250))
            .stroke(Stroke::new(1.0, Color32::from_rgb(225, 225, 225)))
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (label, action) in [
                        ("↻  Rescan", "scan"),
                        ("↑  Parent", "parent"),
                        ("Expand all", "expand"),
                        ("Collapse all", "collapse"),
                        ("Copy path", "copy"),
                        ("Show in Explorer", "explorer"),
                    ] {
                        if ui
                            .add(egui::Button::new(label).frame(false))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            command = Some(action);
                        }
                        if action == "collapse" || action == "explorer" {
                            ui.separator();
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("sort-mode")
                            .selected_text(format!("Sort: {}", sort_label(self.sort)))
                            .show_ui(ui, |ui| {
                                for (sort, label) in [
                                    (SortKey::Size, "Size"),
                                    (SortKey::Name, "Name"),
                                    (SortKey::Modified, "Last modified"),
                                    (SortKey::Recent, "Last accessed"),
                                ] {
                                    if ui.selectable_value(&mut self.sort, sort, label).changed() {
                                        self.rebuild_visible();
                                    }
                                }
                            });
                    });
                });
            });
        match command {
            Some("scan") => self.start_scan(),
            Some("parent") => self.scan_parent(),
            Some("expand") => self.expand_all(),
            Some("collapse") => {
                self.expanded.clear();
                self.expanded.insert(0);
                self.rebuild_visible();
            }
            Some("copy") => self.copy_selected_path(ui.ctx()),
            Some("explorer") => self.open_selected(),
            _ => {}
        }
        ui.add_space(3.0);
        ui.horizontal_wrapped(|ui| {
            summary_card(ui, "TOTAL", human_size(self.tree.nodes[0].size));
            summary_card(ui, "FILES", self.files.to_string());
            summary_card(ui, "FOLDERS", self.directories.to_string());
            summary_card(
                ui,
                "SCAN SPEED",
                if self.scan_in_progress {
                    "Scanning…".to_string()
                } else if self.elapsed.is_zero() {
                    "—".to_string()
                } else {
                    format!(
                        "{:.0} items/s",
                        self.tree.nodes.len() as f64 / self.elapsed.as_secs_f64().max(0.001)
                    )
                },
            );
            summary_card(ui, "INDEX", human_size(self.index_memory as u64));
        });
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            ui.label("Search");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Filter files and folders by name")
                    .desired_width((ui.available_width() - 380.0).max(220.0)),
            );
            let mut changed = response.changed();
            if ui.button("Clear filters").clicked() {
                self.search.clear();
                self.active_extension = None;
                changed = true;
            }
            ui.label(
                RichText::new(format!("{} items", self.visible.len()))
                    .color(Color32::from_rgb(100, 106, 112)),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some((success, message)) = &self.status {
                    let color = if *success {
                        Color32::from_rgb(40, 120, 68)
                    } else {
                        Color32::from_rgb(177, 48, 39)
                    };
                    ui.label(RichText::new(message).small().color(color));
                }
            });
            if changed {
                self.rebuild_visible();
            }
        });
    }

    fn draw_tree(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading("Files and folders");
                if let Some(extension) = &self.active_extension {
                    ui.label(
                        RichText::new(format!("Filtered by {extension}"))
                            .small()
                            .color(ACCENT),
                    );
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.add_sized(
                    [(ui.available_width() - 430.0).max(160.0), 20.0],
                    egui::Label::new(RichText::new("Name").strong()),
                );
                ui.add_sized(
                    [92.0, 20.0],
                    egui::Label::new(RichText::new("Size").strong()),
                );
                ui.add_sized(
                    [64.0, 20.0],
                    egui::Label::new(RichText::new("% parent").strong()),
                );
                ui.add_sized(
                    [54.0, 20.0],
                    egui::Label::new(RichText::new("Items").strong()),
                );
                ui.add_sized(
                    [116.0, 20.0],
                    egui::Label::new(RichText::new("Modified").strong()),
                );
                ui.add_sized(
                    [106.0, 20.0],
                    egui::Label::new(RichText::new("Accessed").strong()),
                );
                ui.add_sized(
                    [76.0, 20.0],
                    egui::Label::new(RichText::new("Type").strong()),
                );
            });
            ui.separator();
            let count = self.visible.len();
            let selected = &mut self.selected;
            let expanded = &mut self.expanded;
            let visible = &self.visible;
            let tree = &self.tree;
            let is_filtered = !self.search.is_empty() || self.active_extension.is_some();
            ScrollArea::both()
                .id_salt("tree")
                .show_rows(ui, 22.0, count, |ui, range| {
                    for index in range {
                        let Some(&(id, depth)) = visible.get(index) else {
                            continue;
                        };
                        let node = tree.nodes[id as usize];
                        let name = tree.name(id).to_string();
                        let is_selected = *selected == id;
                        let fill = if is_selected {
                            ui.visuals().selection.bg_fill
                        } else {
                            Color32::TRANSPARENT
                        };
                        let row = ui.allocate_ui_with_layout(
                            Vec2::new(ui.available_width().max(740.0), 22.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                let row_fill = if is_selected {
                                    fill
                                } else if index % 2 == 0 {
                                    Color32::from_rgb(248, 249, 250)
                                } else {
                                    Color32::TRANSPARENT
                                };
                                ui.painter().rect_filled(ui.max_rect(), 2.0, row_fill);
                                if ui.rect_contains_pointer(ui.max_rect()) && !is_selected {
                                    ui.painter().rect_stroke(
                                        ui.max_rect(),
                                        0.0,
                                        Stroke::new(1.0, Color32::from_rgb(215, 230, 243)),
                                        StrokeKind::Inside,
                                    );
                                }
                                ui.add_space((depth as f32 * 14.0).min(280.0));
                                if node.is_dir {
                                    if is_filtered {
                                        ui.add_sized([18.0, 20.0], egui::Label::new("•"));
                                    } else {
                                        let marker =
                                            if expanded.contains(&id) { "⌄" } else { "›" };
                                        if ui
                                            .add_sized(
                                                [18.0, 20.0],
                                                egui::Button::new(
                                                    RichText::new(marker).color(Color32::GRAY),
                                                )
                                                .frame(false),
                                            )
                                            .clicked()
                                        {
                                            if expanded.contains(&id) {
                                                expanded.remove(&id);
                                            } else {
                                                expanded.insert(id);
                                            }
                                        }
                                    }
                                } else {
                                    ui.add_space(18.0);
                                }
                                let name_width = (ui.available_width() - 510.0).max(150.0);
                                let name = if node.is_dir {
                                    RichText::new(name).strong()
                                } else {
                                    RichText::new(name)
                                };
                                let name_response = ui.add_sized(
                                    [name_width, 20.0],
                                    egui::Button::new(name).frame(false).truncate(),
                                );
                                ui.add_sized(
                                    [92.0, 20.0],
                                    egui::Label::new(
                                        RichText::new(human_size(node.size))
                                            .color(size_color(node.size)),
                                    ),
                                );
                                ui.add_sized(
                                    [64.0, 20.0],
                                    egui::Label::new(format!(
                                        "{:.1}%",
                                        analysis::share_of_parent(tree, id) * 100.0
                                    )),
                                );
                                let items = if node.is_dir { tree.child_count(id) } else { 0 };
                                ui.add_sized([54.0, 20.0], egui::Label::new(items.to_string()));
                                ui.add_sized(
                                    [116.0, 20.0],
                                    egui::Label::new(format_timestamp(node.modified)),
                                );
                                ui.add_sized(
                                    [106.0, 20.0],
                                    egui::Label::new(format_timestamp(node.accessed)),
                                );
                                ui.add_sized(
                                    [76.0, 20.0],
                                    egui::Label::new(if node.is_dir {
                                        "Folder".to_string()
                                    } else {
                                        file_extension(tree.name(id))
                                    }),
                                );
                                name_response.clicked()
                            },
                        );
                        if row.inner {
                            *selected = id;
                        }
                    }
                });
        });
    }

    fn item_path(&self, id: u32) -> PathBuf {
        if id == 0 {
            return self.root.clone();
        }
        let full_path = self.tree.full_path(id);
        let relative = full_path
            .strip_prefix(self.tree.name(0))
            .unwrap_or(&full_path)
            .trim_start_matches(|character| character == '\\' || character == '/');
        self.root.join(relative)
    }

    fn draw_details(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("Item details");
            ui.separator();
            let node = &self.tree.nodes[self.selected as usize];
            let node = *node;
            let name = self.tree.name(self.selected).to_string();
            let path = self.item_path(self.selected);
            let path_text = path.display().to_string();
            ui.label(RichText::new(&name).strong().size(18.0));
            ui.add(egui::Label::new(path_text.clone()).truncate());
            ui.horizontal(|ui| {
                if ui.button("Copy path").clicked() {
                    ui.ctx().copy_text(path_text.clone());
                    self.status = Some((true, "Path copied to clipboard.".to_string()));
                }
                if ui.button("Show in Explorer").clicked() {
                    match reveal_in_explorer(&path, node.is_dir) {
                        Ok(()) => self.status =
                            Some((true, "Opened the selected item in Explorer.".to_string())),
                        Err(error) => {
                            self.status = Some((false, format!("Could not open Explorer: {error}")))
                        }
                    }
                }
            });
            ui.separator();
            detail_row(ui, "Kind", if node.is_dir { "Directory" } else { "File" });
            detail_row(
                ui,
                "Logical size",
                &format!("{} ({} bytes)", human_size(node.size), node.size),
            );
            detail_row(
                ui,
                "Share of parent",
                &format!("{:.2}%", analysis::share_of_parent(&self.tree, self.selected) * 100.0),
            );
            detail_row(
                ui,
                "Share of scan",
                &format!("{:.2}%", analysis::share_of_root(&self.tree, self.selected) * 100.0),
            );
            if node.is_dir {
                detail_row(
                    ui,
                    "Child items",
                    &self.tree.child_count(self.selected).to_string(),
                );
            } else {
                detail_row(ui, "File type", &file_extension(&name));
            }
            detail_row(ui, "Modified", &format_timestamp(node.modified));
            detail_row(ui, "Accessed", &format_timestamp(node.accessed));
            ui.separator();
            ui.label(
                RichText::new("Logical file size is shown. Allocated space and filesystem attributes are not collected.")
                    .small()
                    .color(Color32::GRAY),
            );
            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                ui.label(RichText::new("Read-only scan · files are never modified").color(ACCENT));
            });
        });
    }

    fn draw_treemap(&mut self, ui: &mut egui::Ui) {
        let selected = self.selected;
        let selected_node = self.tree.nodes[selected as usize];
        let focus = if selected_node.is_dir {
            selected
        } else if selected_node.parent != NONE {
            selected_node.parent
        } else {
            0
        };
        if self.treemap_focus != focus {
            self.treemap_focus = focus;
            self.treemap_children =
                analysis::largest_children(&self.tree, focus, MAX_TREEMAP_CHILDREN + 1);
        }
        let name = self.tree.name(focus).to_string();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading("Treemap");
                ui.label(RichText::new(format!("Children of {name}")).color(Color32::GRAY));
            });
            let (rect, _) =
                ui.allocate_exact_size(ui.available_size().max(Vec2::splat(60.0)), Sense::hover());
            ui.painter()
                .rect_filled(rect, 3.0, Color32::from_rgb(246, 247, 248));
            let painter = ui.painter().with_clip_rect(rect.shrink(2.0));
            let mut remaining = self.treemap_children.clone();
            let other = if remaining.len() > MAX_TREEMAP_CHILDREN {
                remaining.pop()
            } else {
                None
            };
            if remaining.is_empty() && other.is_none() {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "No items to display",
                    FontId::proportional(14.0),
                    Color32::GRAY,
                );
                return;
            }
            let mut entries: Vec<(Option<u32>, u64)> = remaining
                .into_iter()
                .map(|id| (Some(id), self.tree.nodes[id as usize].size.max(1)))
                .collect();
            if let Some(other_id) = other {
                let visible_bytes = entries.iter().map(|(_, weight)| *weight).sum::<u64>();
                let other_bytes = self.tree.nodes[focus as usize]
                    .size
                    .saturating_sub(visible_bytes)
                    .max(self.tree.nodes[other_id as usize].size);
                entries.push((None, other_bytes.max(1)));
            }
            let mut hit_targets = Vec::new();
            let weights: Vec<u64> = entries.iter().map(|(_, weight)| *weight).collect();
            let rectangles = partition_treemap(rect, &weights);
            for ((id, _), child_rect) in entries.iter().zip(rectangles) {
                let Some(id) = id else {
                    painter.rect_filled(
                        child_rect.shrink(1.5),
                        2.0,
                        Color32::from_rgb(225, 228, 231),
                    );
                    if child_rect.width() > 48.0 {
                        painter.text(
                            child_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Other items",
                            FontId::proportional(11.0),
                            Color32::from_rgb(75, 82, 90),
                        );
                    }
                    continue;
                };
                draw_treemap_node(
                    &painter,
                    child_rect,
                    &self.tree,
                    *id,
                    0,
                    self.selected,
                    &mut hit_targets,
                );
            }
            for (target, id) in hit_targets {
                let response = ui.interact(target, ui.id().with(("treemap", id)), Sense::click());
                let clicked = response.clicked();
                response.on_hover_text(format!(
                    "{}\n{}\n{:.2}% of scan",
                    self.item_path(id).display(),
                    human_size(self.tree.nodes[id as usize].size),
                    analysis::share_of_root(&self.tree, id) * 100.0
                ));
                if clicked {
                    self.selected = id;
                }
            }
        });
    }

    fn draw_file_types(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("Space by file type");
            ui.label(
                RichText::new("Select a type to filter the tree")
                    .small()
                    .color(Color32::GRAY),
            );
            ui.horizontal(|ui| {
                ui.add_sized([75.0, 18.0], egui::Label::new("Type"));
                ui.add_sized([48.0, 18.0], egui::Label::new("Share"));
                ui.add_sized([72.0, 18.0], egui::Label::new("Size"));
                ui.add_sized([44.0, 18.0], egui::Label::new("Files"));
            });
            ui.separator();
            let max = self
                .file_types
                .first()
                .map(|kind| kind.bytes)
                .unwrap_or(1)
                .max(1);
            let total = self.tree.nodes[0].size;
            let selected_extension = self.active_extension.clone();
            let mut clicked_extension = None;
            ScrollArea::vertical().show(ui, |ui| {
                for kind in &self.file_types {
                    if file_type_row(
                        ui,
                        kind,
                        max,
                        total,
                        selected_extension.as_deref() == Some(kind.extension.as_str()),
                    ) {
                        clicked_extension = Some(kind.extension.clone());
                    }
                }
            });
            if let Some(extension) = clicked_extension {
                self.active_extension = if self.active_extension.as_deref() == Some(&extension) {
                    None
                } else {
                    Some(extension)
                };
                self.rebuild_visible();
            }
        });
    }
}

impl eframe::App for WinBloatApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_scan();
        if self.scan_in_progress {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(243, 243, 243))
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| {
                self.draw_header(ui);
                ui.add_space(7.0);
                let size = ui.available_size();
                let gap = 8.0;
                let bottom_height = (size.y * 0.38).clamp(190.0, (size.y - 230.0).max(190.0));
                let upper_height = (size.y - bottom_height - gap).max(210.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(size.x, upper_height),
                    Layout::left_to_right(Align::Min),
                    |ui| {
                        let width = ui.available_width();
                        let left_width = (width * 0.70).clamp(610.0, width - 300.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(left_width - gap, upper_height),
                            Layout::top_down(Align::Min),
                            |ui| {
                                card_frame().show(ui, |ui| {
                                    ui.set_min_size(ui.available_size());
                                    self.draw_tree(ui);
                                });
                            },
                        );
                        ui.add_space(gap);
                        ui.allocate_ui_with_layout(
                            Vec2::new(width - left_width, upper_height),
                            Layout::top_down(Align::Min),
                            |ui| {
                                card_frame().show(ui, |ui| {
                                    ui.set_min_size(ui.available_size());
                                    self.draw_details(ui);
                                });
                            },
                        );
                    },
                );
                ui.add_space(gap);
                ui.allocate_ui_with_layout(
                    Vec2::new(size.x, bottom_height),
                    Layout::left_to_right(Align::Min),
                    |ui| {
                        let width = ui.available_width();
                        let left_width = width * 0.68;
                        ui.allocate_ui_with_layout(
                            Vec2::new(left_width - gap, bottom_height),
                            Layout::top_down(Align::Min),
                            |ui| {
                                card_frame().show(ui, |ui| {
                                    ui.set_min_size(ui.available_size());
                                    self.draw_treemap(ui);
                                });
                            },
                        );
                        ui.add_space(gap);
                        ui.allocate_ui_with_layout(
                            Vec2::new(width - left_width, bottom_height),
                            Layout::top_down(Align::Min),
                            |ui| {
                                card_frame().show(ui, |ui| {
                                    ui.set_min_size(ui.available_size());
                                    self.draw_file_types(ui);
                                });
                            },
                        );
                    },
                );
            });
    }
}

fn summary_card(ui: &mut egui::Ui, label: &str, value: String) {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, Color32::from_rgb(215, 215, 215)))
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.label(
                RichText::new(label)
                    .small()
                    .color(Color32::from_rgb(100, 106, 112)),
            );
            ui.label(RichText::new(value).strong());
        });
}

fn sort_label(sort: SortKey) -> &'static str {
    match sort {
        SortKey::Size => "Size",
        SortKey::Name => "Name",
        SortKey::Recent => "Last accessed",
        SortKey::Modified => "Last modified",
    }
}

fn detail_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [108.0, 20.0],
            egui::Label::new(RichText::new(label).color(Color32::GRAY)),
        );
        ui.label(value);
    });
}

fn file_type_row(
    ui: &mut egui::Ui,
    kind: &FileTypeStats,
    max: u64,
    total: u64,
    selected: bool,
) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        clicked = ui
            .add_sized(
                [75.0, 22.0],
                egui::Button::new(&kind.extension)
                    .selected(selected)
                    .frame(false),
            )
            .clicked();
        let share = if total == 0 {
            0.0
        } else {
            kind.bytes as f64 / total as f64 * 100.0
        };
        ui.add_sized([48.0, 20.0], egui::Label::new(format!("{share:.1}%")));
        ui.add_sized([72.0, 20.0], egui::Label::new(human_size(kind.bytes)));
        ui.add_sized([44.0, 20.0], egui::Label::new(kind.files.to_string()));
        let fraction = kind.bytes as f32 / max as f32;
        let bar_width = ui.available_width().clamp(20.0, 90.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(bar_width, 7.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, 1.0, Color32::from_rgb(228, 231, 234));
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(rect.width() * fraction, rect.height())),
            1.0,
            ACCENT,
        );
    });
    clicked
}

fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, Color32::from_rgb(205, 205, 205)))
        .inner_margin(egui::Margin::symmetric(10, 8))
}

fn configure_style(context: &egui::Context, windows_font: Vec<u8>) {
    let mut fonts = egui::FontDefinitions::default();
    if !windows_font.is_empty() {
        fonts.font_data.insert(
            "windows-segoe-ui".to_string(),
            egui::FontData::from_owned(windows_font).into(),
        );
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "windows-segoe-ui".to_string());
    }
    context.set_fonts(fonts);
    context.set_theme(egui::Theme::Light);

    let mut style = (*context.style_of(egui::Theme::Light)).clone();
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(20.0, FontFamily::Proportional),
        ),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.indent = 18.0;
    style.visuals = egui::Visuals::light();
    let visuals = &mut style.visuals;
    visuals.panel_fill = Color32::from_rgb(243, 243, 243);
    visuals.window_fill = Color32::WHITE;
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.faint_bg_color = Color32::from_rgb(248, 248, 248);
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(0, 86, 160));
    visuals.widgets.inactive.bg_fill = Color32::WHITE;
    visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(205, 205, 205));
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(235, 243, 250);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(133, 183, 224));
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(243, 243, 243);
    context.set_style_of(egui::Theme::Light, style);
}

fn load_app_icon() -> io::Result<egui::IconData> {
    let icon = image::load_from_memory(include_bytes!("../assets/icon.ico"))
        .map_err(|error| io::Error::other(format!("Could not load application icon: {error}")))?
        .into_rgba8();
    let (width, height) = icon.dimensions();
    Ok(egui::IconData {
        rgba: icon.into_raw(),
        width,
        height,
    })
}

#[cfg(windows)]
fn load_windows_font() -> io::Result<Vec<u8>> {
    let windows_dir = std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into());
    std::fs::read(PathBuf::from(windows_dir).join("Fonts").join("segoeui.ttf")).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("Could not load the Windows Segoe UI font: {error}"),
        )
    })
}

#[cfg(not(windows))]
fn load_windows_font() -> io::Result<Vec<u8>> {
    Ok(Vec::new())
}

fn file_extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy().to_lowercase()))
        .unwrap_or_else(|| "(no extension)".to_string())
}

fn display_root_path(root: &Path) -> String {
    let display = root.to_string_lossy();
    #[cfg(windows)]
    if let Some(path) = display.strip_prefix(r"\\?\") {
        return path.to_string();
    }
    display.into_owned()
}

#[cfg(windows)]
fn reveal_in_explorer(path: &Path, is_dir: bool) -> io::Result<()> {
    let mut command = std::process::Command::new("explorer.exe");
    if is_dir {
        command.arg(path);
    } else {
        command.arg("/select,").arg(path);
    }
    command.spawn().map(|_| ())
}

#[cfg(not(windows))]
fn reveal_in_explorer(_path: &Path, _is_dir: bool) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Explorer is available only on Windows",
    ))
}

fn draw_treemap_node(
    painter: &egui::Painter,
    rect: egui::Rect,
    tree: &Tree,
    id: u32,
    depth: usize,
    selected: u32,
    hit_targets: &mut Vec<(Rect, u32)>,
) {
    let node = &tree.nodes[id as usize];
    let tile = rect.shrink(1.5);
    hit_targets.push((tile, id));
    let color = size_color(node.size);
    painter.rect_filled(tile, 2.0, color);
    let stroke = if id == selected {
        Stroke::new(2.0_f32, ACCENT)
    } else {
        Stroke::new(1.0_f32, Color32::WHITE)
    };
    painter.rect_stroke(tile, 2.0, stroke, StrokeKind::Inside);
    if tile.width() > 58.0 && tile.height() > 20.0 {
        painter.text(
            tile.left_top() + Vec2::new(4.0, 2.0),
            egui::Align2::LEFT_TOP,
            tree.name(id),
            FontId::proportional(13.0),
            Color32::WHITE,
        );
        if tile.width() > 82.0 && tile.height() > 38.0 {
            painter.text(
                tile.left_bottom() - Vec2::new(-4.0, 3.0),
                egui::Align2::LEFT_BOTTOM,
                human_size(node.size),
                FontId::proportional(12.0),
                Color32::WHITE,
            );
        }
    }
    if node.is_dir && depth < 1 && tile.width() > 120.0 && tile.height() > 70.0 {
        let children = analysis::largest_children(tree, id, 4);
        let displayed_bytes = children
            .iter()
            .map(|child| tree.nodes[*child as usize].size)
            .sum::<u64>();
        let mut entries: Vec<(Option<u32>, u64)> = children
            .into_iter()
            .map(|child| (Some(child), tree.nodes[child as usize].size.max(1)))
            .collect();
        let other_bytes = node.size.saturating_sub(displayed_bytes);
        if other_bytes > 0 {
            entries.push((None, other_bytes));
        }
        let content_rect = Rect::from_min_max(
            egui::pos2(tile.left(), tile.top() + 20.0),
            tile.right_bottom(),
        );
        let weights: Vec<u64> = entries.iter().map(|(_, weight)| *weight).collect();
        for ((child, _), child_rect) in entries
            .iter()
            .zip(partition_treemap(content_rect, &weights))
        {
            if let Some(child) = child {
                draw_treemap_node(
                    painter,
                    child_rect,
                    tree,
                    *child,
                    depth + 1,
                    selected,
                    hit_targets,
                );
            } else {
                painter.rect_filled(
                    child_rect.shrink(1.5),
                    2.0,
                    Color32::from_rgb(225, 228, 231),
                );
                if child_rect.width() > 48.0 {
                    painter.text(
                        child_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Other items",
                        FontId::proportional(12.0),
                        Color32::from_rgb(75, 82, 90),
                    );
                }
            }
        }
    }
}

fn size_color(size: u64) -> Color32 {
    match size_tier(size) {
        3 => Color32::from_rgb(201, 91, 55),
        2 => Color32::from_rgb(210, 157, 48),
        1 => Color32::from_rgb(53, 139, 91),
        _ => Color32::from_rgb(55, 119, 176),
    }
}

fn partition_treemap(rect: Rect, weights: &[u64]) -> Vec<Rect> {
    fn split(rect: Rect, weights: &[u64], output: &mut [Rect]) {
        if weights.len() == 1 {
            output[0] = rect;
            return;
        }

        let total = weights.iter().map(|weight| *weight as f64).sum::<f64>();
        if total == 0.0 {
            let divider = if rect.width() >= rect.height() {
                rect.left() + rect.width() / weights.len() as f32
            } else {
                rect.top() + rect.height() / weights.len() as f32
            };
            let (first, second) = output.split_at_mut(1);
            if rect.width() >= rect.height() {
                split(
                    Rect::from_min_max(rect.min, egui::pos2(divider, rect.bottom())),
                    &weights[..1],
                    first,
                );
                split(
                    Rect::from_min_max(egui::pos2(divider, rect.top()), rect.max),
                    &weights[1..],
                    second,
                );
            } else {
                split(
                    Rect::from_min_max(rect.min, egui::pos2(rect.right(), divider)),
                    &weights[..1],
                    first,
                );
                split(
                    Rect::from_min_max(egui::pos2(rect.left(), divider), rect.max),
                    &weights[1..],
                    second,
                );
            }
            return;
        }

        let half = total / 2.0;
        let mut left_weight = 0.0;
        let mut split_at = 1;
        for (index, weight) in weights.iter().enumerate().take(weights.len() - 1) {
            left_weight += *weight as f64;
            split_at = index + 1;
            if left_weight >= half {
                break;
            }
        }
        let left_weight = weights[..split_at]
            .iter()
            .map(|weight| *weight as f64)
            .sum::<f64>();
        let fraction = (left_weight / total) as f32;
        let (first, second) = output.split_at_mut(split_at);
        if rect.width() >= rect.height() {
            let divider = rect.left() + rect.width() * fraction;
            split(
                Rect::from_min_max(rect.min, egui::pos2(divider, rect.bottom())),
                &weights[..split_at],
                first,
            );
            split(
                Rect::from_min_max(egui::pos2(divider, rect.top()), rect.max),
                &weights[split_at..],
                second,
            );
        } else {
            let divider = rect.top() + rect.height() * fraction;
            split(
                Rect::from_min_max(rect.min, egui::pos2(rect.right(), divider)),
                &weights[..split_at],
                first,
            );
            split(
                Rect::from_min_max(egui::pos2(rect.left(), divider), rect.max),
                &weights[split_at..],
                second,
            );
        }
    }

    let mut rectangles = vec![Rect::from_min_size(rect.min, Vec2::ZERO); weights.len()];
    if !weights.is_empty() {
        split(rect, weights, &mut rectangles);
    }
    rectangles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter_fixture() -> Tree {
        Tree {
            names: "rootfoldernote.txtimage.png".to_string(),
            nodes: vec![
                Node {
                    name_start: 0,
                    name_len: 4,
                    parent: NONE,
                    first_child: 1,
                    next_sibling: NONE,
                    size: 300,
                    modified: 0,
                    accessed: 0,
                    is_dir: true,
                },
                Node {
                    name_start: 4,
                    name_len: 6,
                    parent: 0,
                    first_child: 2,
                    next_sibling: 3,
                    size: 200,
                    modified: 0,
                    accessed: 0,
                    is_dir: true,
                },
                Node {
                    name_start: 10,
                    name_len: 8,
                    parent: 1,
                    first_child: NONE,
                    next_sibling: NONE,
                    size: 150,
                    modified: 0,
                    accessed: 0,
                    is_dir: false,
                },
                Node {
                    name_start: 18,
                    name_len: 9,
                    parent: 0,
                    first_child: NONE,
                    next_sibling: NONE,
                    size: 100,
                    modified: 0,
                    accessed: 0,
                    is_dir: false,
                },
            ],
        }
    }

    #[test]
    fn extension_filter_keeps_ancestor_folders_and_excludes_other_types() {
        let mut app = WinBloatApp::new(
            filter_fixture(),
            PathBuf::from(r"C:\data"),
            Duration::from_secs(1),
        );
        app.active_extension = Some(".txt".to_string());
        app.rebuild_visible();

        assert_eq!(
            app.visible.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(app.item_path(2), PathBuf::from(r"C:\data\folder\note.txt"));
    }

    #[test]
    fn extension_matching_is_case_insensitive_and_handles_extensionless_files() {
        assert_eq!(file_extension("ARCHIVE.TAR.GZ"), ".gz");
        assert_eq!(file_extension("README"), "(no extension)");
    }

    #[test]
    fn application_icon_decodes_to_a_valid_rgba_image() {
        let icon = load_app_icon().expect("application icon should decode");
        assert!(icon.width >= 16);
        assert!(icon.height >= 16);
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
    }

    #[test]
    fn treemap_tiles_preserve_each_entrys_area_share() {
        let bounds = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(400.0, 200.0));
        let tiles = partition_treemap(bounds, &[1, 3]);

        assert_eq!(tiles.len(), 2);
        assert!((tiles[0].area() / bounds.area() - 0.25).abs() < 0.001);
        assert!((tiles[1].area() / bounds.area() - 0.75).abs() < 0.001);
        assert!((tiles[0].intersect(tiles[1]).area()) == 0.0);
    }

    #[cfg(windows)]
    #[test]
    fn windows_segoe_ui_font_is_installed_and_readable() {
        assert!(!load_windows_font().unwrap().is_empty());
    }
}
