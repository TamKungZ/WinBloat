use crate::analysis::{self, FileTypeStats};
use crate::format::{format_timestamp, human_size, size_tier};
use crate::scanner;
use crate::tree::{Node, SortKey, Tree, NONE};
use eframe::egui::{
    self, Align, Color32, FontId, Layout, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind,
    Vec2,
};
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const WINDOW_SIZE: [f32; 2] = [1440.0, 900.0];
const MAX_TREEMAP_CHILDREN: usize = 8;

pub fn run(root: &Path) -> io::Result<()> {
    let start = Instant::now();
    let tree = scanner::scan(root)?;
    let elapsed = start.elapsed();
    let app = WinBloatApp::new(tree, root.to_path_buf(), elapsed);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(WINDOW_SIZE)
            .with_min_inner_size([1000.0, 650.0]),
        ..Default::default()
    };
    eframe::run_native("WinBloat", options, Box::new(|_| Ok(Box::new(app))))
        .map_err(|error| io::Error::other(error.to_string()))
}

struct WinBloatApp {
    tree: Tree,
    root: PathBuf,
    elapsed: Duration,
    expanded: HashSet<u32>,
    visible: Vec<(u32, usize)>,
    selected: u32,
    search: String,
    sort: SortKey,
    file_types: Vec<FileTypeStats>,
    files: usize,
    directories: usize,
    index_memory: usize,
    treemap_focus: u32,
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
            root,
            elapsed,
            expanded: HashSet::from([0]),
            visible: Vec::new(),
            selected: 0,
            search: String::new(),
            sort: SortKey::Size,
            file_types,
            files,
            directories,
            index_memory,
            treemap_focus: 0,
            treemap_children,
        };
        app.rebuild_visible();
        app
    }

    fn rebuild_visible(&mut self) {
        let mut visible = Vec::new();
        if self.search.is_empty() {
            Self::collect_visible(&self.tree, &self.expanded, self.sort, 0, 0, &mut visible);
        } else {
            let query = self.search.to_lowercase();
            for (id, _) in self.tree.nodes.iter().enumerate() {
                if self.tree.name(id as u32).to_lowercase().contains(&query) {
                    visible.push((id as u32, node_depth(&self.tree, id as u32)));
                }
            }
            visible.sort_unstable_by(|(left, _), (right, _)| {
                self.tree
                    .name(*left)
                    .to_lowercase()
                    .cmp(&self.tree.name(*right).to_lowercase())
            });
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

    fn draw_header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("summary").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("WinBloat").strong());
                ui.label(RichText::new("Read-only disk analysis").color(Color32::LIGHT_BLUE));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(format!("Scanned in {:.2?}", self.elapsed));
                });
            });
            ui.add(egui::Label::new(self.root.display().to_string()).truncate());

            ui.horizontal_wrapped(|ui| {
                summary_card(ui, "Logical size", human_size(self.tree.nodes[0].size));
                summary_card(ui, "Files", self.files.to_string());
                summary_card(ui, "Directories", self.directories.to_string());
                summary_card(
                    ui,
                    "Items/sec",
                    format!(
                        "{:.0}",
                        self.tree.nodes.len() as f64 / self.elapsed.as_secs_f64().max(0.001)
                    ),
                );
                summary_card(ui, "Index memory", human_size(self.index_memory as u64));
            });

            ui.horizontal(|ui| {
                ui.label("Search:");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("File or folder name")
                        .desired_width(250.0),
                );
                if response.changed() {
                    self.rebuild_visible();
                }
                ui.label("Sort:");
                let selected_sort = match self.sort {
                    SortKey::Size => "Size",
                    SortKey::Name => "Name",
                    SortKey::Recent => "Last accessed",
                    SortKey::Modified => "Last modified",
                };
                let mut sort_changed = false;
                egui::ComboBox::from_id_salt("sort-mode")
                    .selected_text(selected_sort)
                    .show_ui(ui, |ui| {
                        for (sort, label) in [
                            (SortKey::Size, "Size"),
                            (SortKey::Name, "Name"),
                            (SortKey::Modified, "Last modified"),
                            (SortKey::Recent, "Last accessed"),
                        ] {
                            sort_changed |=
                                ui.selectable_value(&mut self.sort, sort, label).changed();
                        }
                    });
                if sort_changed {
                    self.rebuild_visible();
                }
                if !self.search.is_empty() {
                    ui.label(format!("{} matches", self.visible.len()));
                }
            });
        });
    }

    fn draw_tree(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("Files and folders");
            ui.separator();
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.add_sized(
                    [92.0, 20.0],
                    egui::Label::new(RichText::new("Size").strong()),
                );
                ui.add_sized(
                    [58.0, 20.0],
                    egui::Label::new(RichText::new("% parent").strong()),
                );
                ui.add_sized(
                    [54.0, 20.0],
                    egui::Label::new(RichText::new("Items").strong()),
                );
                ui.add_sized(
                    [100.0, 20.0],
                    egui::Label::new(RichText::new("Modified").strong()),
                );
                ui.add_sized(
                    [100.0, 20.0],
                    egui::Label::new(RichText::new("Accessed").strong()),
                );
                ui.label(RichText::new("Name").strong());
            });
            ui.separator();
            let count = self.visible.len();
            let selected = &mut self.selected;
            let expanded = &mut self.expanded;
            let visible = &self.visible;
            let tree = &self.tree;
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
                                ui.painter().rect_filled(ui.max_rect(), 2.0, fill);
                                ui.add_space((depth as f32 * 14.0).min(280.0));
                                if node.is_dir {
                                    let marker = if expanded.contains(&id) { "v" } else { ">" };
                                    if ui
                                        .add_sized(
                                            [20.0, 20.0],
                                            egui::Button::new(marker).frame(false),
                                        )
                                        .clicked()
                                    {
                                        if expanded.contains(&id) {
                                            expanded.remove(&id);
                                        } else {
                                            expanded.insert(id);
                                        }
                                    }
                                } else {
                                    ui.add_space(20.0);
                                }
                                ui.add_sized(
                                    [92.0, 20.0],
                                    egui::Label::new(
                                        RichText::new(human_size(node.size))
                                            .color(size_color(node.size)),
                                    ),
                                );
                                ui.add_sized(
                                    [58.0, 20.0],
                                    egui::Label::new(format!(
                                        "{:.1}%",
                                        analysis::share_of_parent(tree, id) * 100.0
                                    )),
                                );
                                let items = if node.is_dir { tree.child_count(id) } else { 0 };
                                ui.add_sized([54.0, 20.0], egui::Label::new(items.to_string()));
                                ui.add_sized(
                                    [100.0, 20.0],
                                    egui::Label::new(format_timestamp(node.modified)),
                                );
                                ui.add_sized(
                                    [100.0, 20.0],
                                    egui::Label::new(format_timestamp(node.accessed)),
                                );
                                let name = if node.is_dir {
                                    RichText::new(name).strong()
                                } else {
                                    RichText::new(name)
                                };
                                ui.selectable_label(is_selected, name).clicked()
                            },
                        );
                        if row.inner {
                            *selected = id;
                        }
                    }
                });
        });
    }

    fn draw_details(&self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("Selected item");
            ui.separator();
            let node = &self.tree.nodes[self.selected as usize];
            let name = self.tree.name(self.selected);
            ui.label(RichText::new(name).strong().size(18.0));
            ui.label(self.tree.full_path(self.selected));
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
                    "Immediate items",
                    &self.tree.child_count(self.selected).to_string(),
                );
            } else {
                let extension = Path::new(name)
                    .extension()
                    .map(|value| format!(".{}", value.to_string_lossy()))
                    .unwrap_or_else(|| "(no extension)".to_string());
                detail_row(ui, "File type", &extension);
            }
            detail_row(ui, "Modified", &format_timestamp(node.modified));
            detail_row(ui, "Accessed", &format_timestamp(node.accessed));
            ui.separator();
            ui.label(
                RichText::new("Size is logical file size. Allocated disk space and filesystem attributes are not queried.")
                    .small()
                    .color(Color32::GRAY),
            );
            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                ui.label(RichText::new("Read-only: WinBloat does not modify scanned files.").color(Color32::LIGHT_BLUE));
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
            ui.painter().rect_filled(rect, 3.0, Color32::from_gray(28));
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
            let total = entries
                .iter()
                .map(|(_, weight)| *weight)
                .sum::<u64>()
                .max(1);
            let mut offset = 0.0;
            for (index, (id, weight)) in entries.iter().enumerate() {
                let fraction = *weight as f32 / total as f32;
                let child_rect = if index + 1 == entries.len() {
                    Rect::from_min_max(
                        egui::pos2(rect.left() + offset, rect.top()),
                        rect.right_bottom(),
                    )
                } else {
                    let next = (offset + rect.width() * fraction).min(rect.right());
                    let child = Rect::from_min_max(
                        egui::pos2(rect.left() + offset, rect.top()),
                        egui::pos2(next, rect.bottom()),
                    );
                    offset += child.width();
                    child
                };
                let Some(id) = id else {
                    painter.rect_filled(child_rect.shrink(1.5), 2.0, Color32::from_gray(72));
                    if child_rect.width() > 48.0 {
                        painter.text(
                            child_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Other items",
                            FontId::proportional(11.0),
                            Color32::WHITE,
                        );
                    }
                    continue;
                };
                draw_treemap_node(&painter, child_rect, &self.tree, *id, 0, self.selected);
                let response =
                    ui.interact(child_rect, ui.id().with(("treemap", id)), Sense::click());
                let clicked = response.clicked();
                response.on_hover_text(format!(
                    "{}\n{}\n{:.2}% of scan",
                    self.tree.full_path(*id),
                    human_size(self.tree.nodes[*id as usize].size),
                    analysis::share_of_root(&self.tree, *id) * 100.0
                ));
                if clicked {
                    self.selected = *id;
                }
            }
        });
    }

    fn draw_file_types(&self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("File types");
            ui.separator();
            let max = self
                .file_types
                .first()
                .map(|kind| kind.bytes)
                .unwrap_or(1)
                .max(1);
            ScrollArea::vertical().show(ui, |ui| {
                for kind in self.file_types.iter().take(12) {
                    file_type_row(ui, kind, max, self.tree.nodes[0].size);
                }
            });
        });
    }
}

impl eframe::App for WinBloatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.draw_header(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            let size = ui.available_size();
            let upper_height = (size.y * 0.64).max(250.0);
            ui.allocate_ui_with_layout(
                Vec2::new(size.x, upper_height),
                Layout::left_to_right(Align::Min),
                |ui| {
                    let left_width = (ui.available_width() * 0.67).max(520.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(left_width, upper_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            egui::Frame::group(ui.style()).show(ui, |ui| self.draw_tree(ui));
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), upper_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            egui::Frame::group(ui.style()).show(ui, |ui| self.draw_details(ui));
                        },
                    );
                },
            );
            ui.add_space(6.0);
            let bottom_height = (ui.available_height() - 6.0).max(150.0);
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), bottom_height),
                Layout::left_to_right(Align::Min),
                |ui| {
                    let left_width = ui.available_width() * 0.68;
                    ui.allocate_ui_with_layout(
                        Vec2::new(left_width, bottom_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            egui::Frame::group(ui.style()).show(ui, |ui| self.draw_treemap(ui));
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), bottom_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            egui::Frame::group(ui.style()).show(ui, |ui| self.draw_file_types(ui));
                        },
                    );
                },
            );
        });
    }
}

fn summary_card(ui: &mut egui::Ui, label: &str, value: String) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new(label).small().color(Color32::GRAY));
        ui.label(RichText::new(value).strong());
    });
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

fn file_type_row(ui: &mut egui::Ui, kind: &FileTypeStats, max: u64, total: u64) {
    ui.horizontal(|ui| {
        ui.add_sized([95.0, 20.0], egui::Label::new(&kind.extension));
        let share = if total == 0 {
            0.0
        } else {
            kind.bytes as f64 / total as f64 * 100.0
        };
        ui.add_sized([50.0, 20.0], egui::Label::new(format!("{share:.1}%")));
        ui.label(human_size(kind.bytes));
        ui.small(format!("{} files", kind.files));
        let fraction = kind.bytes as f32 / max as f32;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(45.0, 8.0), Sense::hover());
        ui.painter().rect_filled(rect, 1.0, Color32::from_gray(50));
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(rect.width() * fraction, rect.height())),
            1.0,
            Color32::LIGHT_BLUE,
        );
    });
}

fn draw_treemap_node(
    painter: &egui::Painter,
    rect: egui::Rect,
    tree: &Tree,
    id: u32,
    depth: usize,
    selected: u32,
) {
    let node = &tree.nodes[id as usize];
    let tile = rect.shrink(1.5);
    let color = size_color(node.size);
    painter.rect_filled(tile, 2.0, color);
    let stroke = if id == selected {
        Stroke::new(2.0_f32, Color32::WHITE)
    } else {
        Stroke::new(1.0_f32, Color32::from_gray(24))
    };
    painter.rect_stroke(tile, 2.0, stroke, StrokeKind::Inside);
    if tile.width() > 58.0 && tile.height() > 20.0 {
        painter.text(
            tile.left_top() + Vec2::new(4.0, 2.0),
            egui::Align2::LEFT_TOP,
            tree.name(id),
            FontId::proportional(11.0),
            Color32::WHITE,
        );
        if tile.width() > 82.0 && tile.height() > 38.0 {
            painter.text(
                tile.left_bottom() - Vec2::new(-4.0, 3.0),
                egui::Align2::LEFT_BOTTOM,
                human_size(node.size),
                FontId::proportional(10.0),
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
        let total = entries
            .iter()
            .map(|(_, weight)| *weight)
            .sum::<u64>()
            .max(1);
        let mut left = tile.left();
        for (index, (child, weight)) in entries.iter().enumerate() {
            let next = if index + 1 == entries.len() {
                tile.right()
            } else {
                (left + tile.width() * *weight as f32 / total as f32).min(tile.right())
            };
            let child_rect = egui::Rect::from_min_max(
                egui::pos2(left, tile.top() + 20.0),
                egui::pos2(next, tile.bottom()),
            );
            left = next;
            if let Some(child) = child {
                draw_treemap_node(painter, child_rect, tree, *child, depth + 1, selected);
            } else {
                painter.rect_filled(child_rect.shrink(1.5), 2.0, Color32::from_gray(72));
                if child_rect.width() > 48.0 {
                    painter.text(
                        child_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Other items",
                        FontId::proportional(10.0),
                        Color32::WHITE,
                    );
                }
            }
        }
    }
}

fn node_depth(tree: &Tree, mut id: u32) -> usize {
    let mut depth = 0;
    while tree.nodes[id as usize].parent != NONE {
        depth += 1;
        id = tree.nodes[id as usize].parent;
    }
    depth
}

fn size_color(size: u64) -> Color32 {
    match size_tier(size) {
        3 => Color32::from_rgb(174, 64, 58),
        2 => Color32::from_rgb(176, 135, 47),
        1 => Color32::from_rgb(56, 135, 91),
        _ => Color32::from_rgb(53, 112, 155),
    }
}
