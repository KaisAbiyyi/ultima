#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Native-only, offline visual implementation of the supplied Codex wrapper UI.
//! This prototype does not issue turns, execute tools, or grant permissions.
mod engine_probe;
mod icons;
mod theme;

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Frame, Key, Layout, Margin, RichText, ScrollArea,
    Sense, Stroke, TextEdit, Vec2,
};
use engine_probe::{EngineProbe, ProbeStatus};
use icons::Icon;
use std::path::PathBuf;
use theme::{
    body, mono, small, BORDER, CANVAS, GREEN, HOVER, MUTED, RAISED, SECONDARY, SURFACE, TEXT,
};

const MAX_HISTORY_ROWS: usize = 10_000;
const MAX_DRAFT_CHARS: usize = 8 * 1024;
const PROJECTS: [&str; 3] = ["codex-wrapper-ui", "pty-sandbox-daemon", "ultima"];
const PROJECT_THREADS: [[&str; 2]; 3] = [
    [
        "Zero-copy IPC line framing",
        "Minimal dark monochrome tokens",
    ],
    ["Read-only sandbox profile", "Process-tree ownership"],
    ["NC-10 renderer feasibility", "Session gate extraction"],
];
const CHAT_TITLES: [&str; 2] = ["Regex semver git tag filter", "SQLite WAL pragma tuning"];
const REFERENCE_PATCH: &str = r#"import { StringDecoder } from 'node:string_decoder';

export function createJsonLineParser(onFrame: (msg: unknown) => void) {
  const decoder = new StringDecoder('utf8');
  let carry = '';

  return (chunk: Buffer) => {
    carry += decoder.write(chunk);
    const lines = carry.split('\n');
    carry = lines.pop() ?? '';

    for (const line of lines) {
      if (!line.trim()) continue;
      onFrame(JSON.parse(line));
    }
  };
}"#;

// Egui performs layout and painting; these compact native rows reproduce the
// reference's 28px ghost-navigation rows, rather than pill-shaped buttons.
fn ghost_row(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    indent: f32,
    disclosure: Option<bool>,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 27.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
    });
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(8),
            Color32::from_white_alpha(if selected { 14 } else { 7 }),
        );
    }
    let color = if selected {
        TEXT
    } else if response.hovered() {
        SECONDARY
    } else {
        MUTED
    };
    if let Some(expanded) = disclosure {
        let c = rect.left_center() + egui::vec2(14.0, 0.0);
        let stroke = Stroke::new(1.35, MUTED);
        let points = if expanded {
            [
                c + egui::vec2(-3.0, -1.5),
                c + egui::vec2(0.0, 1.5),
                c + egui::vec2(3.0, -1.5),
            ]
        } else {
            [
                c + egui::vec2(-1.5, -3.0),
                c + egui::vec2(1.5, 0.0),
                c + egui::vec2(-1.5, 3.0),
            ]
        };
        ui.painter().line_segment([points[0], points[1]], stroke);
        ui.painter().line_segment([points[1], points[2]], stroke);
    } else if indent > 0.0 {
        let x = rect.left() + 15.0;
        ui.painter().line_segment(
            [
                egui::pos2(x, rect.top() - 1.0),
                egui::pos2(x, rect.bottom() + 1.0),
            ],
            Stroke::new(1.0, BORDER),
        );
    }
    ui.painter().text(
        rect.left_center()
            + egui::vec2(
                if disclosure.is_some() {
                    28.0
                } else {
                    9.0 + indent
                },
                0.0,
            ),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(11.7),
        color,
    );
    if selected && disclosure.is_none() {
        ui.painter()
            .circle_filled(rect.right_center() - egui::vec2(9.0, 0.0), 2.1, TEXT);
    }
    response
}

fn centered_width(ui: &mut egui::Ui, max_width: f32, add: impl FnOnce(&mut egui::Ui)) {
    let total = ui.available_width();
    let content = total.min(max_width).max(1.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(((total - content) * 0.5).max(0.0));
        ui.allocate_ui_with_layout(Vec2::new(content, 0.0), Layout::top_down(Align::Min), add);
    });
}

struct ChatRow {
    from_user: bool,
    preview: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    None,
    Search,
    Settings,
}

struct RendererSpike {
    project: usize,
    selected_thread: usize,
    expanded: [bool; 3],
    standalone: bool,
    sidebar_collapsed: bool,
    overlay: Overlay,
    search: String,
    draft: String,
    history: Vec<ChatRow>,
    show_reference: bool,
    engine: Option<EngineProbe>,
    connected_project: Option<String>,
    window_title: String,
}

impl RendererSpike {
    fn new(
        history_rows: usize,
        show_reference: bool,
        engine: Option<EngineProbe>,
        project: Option<String>,
    ) -> Self {
        let history = (0..history_rows.min(MAX_HISTORY_ROWS))
            .map(|i| ChatRow {
                from_user: i % 2 == 0,
                preview: format!(
                    "Fixture message {}: bounded read-only renderer workload; no Codex execution.",
                    i + 1
                ),
            })
            .collect();
        Self {
            project: 0,
            selected_thread: 0,
            expanded: [true, false, false],
            standalone: false,
            sidebar_collapsed: false,
            overlay: Overlay::None,
            search: String::new(),
            draft: String::new(),
            history,
            show_reference,
            engine,
            connected_project: project,
            window_title: String::new(),
        }
    }

    fn engine_state(&self) -> &'static str {
        match self.engine.as_ref().map(EngineProbe::status) {
            Some(ProbeStatus::Connecting) => "APP SERVER CONNECTING",
            Some(ProbeStatus::Ready) => "APP SERVER READY",
            Some(ProbeStatus::Failed) => "APP SERVER FAILED",
            Some(ProbeStatus::Stopped) => "APP SERVER STOPPED",
            None => "RENDERER ONLY",
        }
    }

    fn active_project(&self) -> &str {
        if self.standalone {
            "standalone"
        } else {
            self.connected_project
                .as_deref()
                .unwrap_or(PROJECTS[self.project])
        }
    }

    fn active_title(&self) -> &str {
        if self.standalone {
            CHAT_TITLES[self.selected_thread.min(CHAT_TITLES.len() - 1)]
        } else {
            PROJECT_THREADS[self.project][self.selected_thread.min(1)]
        }
    }

    fn new_preview(&mut self) {
        // Local-only interaction; no fake Codex thread or approval is created.
        self.draft.clear();
        self.show_reference = false;
        self.selected_thread = 0;
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        ctx.input_mut(|input| {
            if input.consume_key(egui::Modifiers::COMMAND, Key::B) {
                self.sidebar_collapsed = !self.sidebar_collapsed;
            }
            if input.consume_key(egui::Modifiers::COMMAND, Key::K) {
                self.overlay = Overlay::Search;
            }
            if input.consume_key(egui::Modifiers::COMMAND, Key::N) {
                self.new_preview();
            }
            if input.consume_key(egui::Modifiers::NONE, Key::Escape) {
                self.overlay = Overlay::None;
            }
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let width = if self.sidebar_collapsed { 42.0 } else { 238.0 };
        let available_h = ui.available_height() - 2.0;
        theme::panel().show(ui, |ui| {
            ui.set_min_height(available_h - 25.0);
            ui.set_width(width);
            if self.sidebar_collapsed {
                ui.vertical_centered(|ui| {
                    if icons::button(ui, Icon::Sidebar, "Expand sidebar (Ctrl+B)").clicked() {
                        self.sidebar_collapsed = false;
                    }
                    ui.add_space(14.0);
                    if icons::button(ui, Icon::Add, "New local preview (Ctrl+N)").clicked() {
                        self.new_preview();
                    }
                    if icons::button(ui, Icon::Search, "Search (Ctrl+K)").clicked() {
                        self.overlay = Overlay::Search;
                    }
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    if icons::button(ui, Icon::Settings, "Settings").clicked() {
                        self.overlay = Overlay::Settings;
                    }
                });
                return;
            }

            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 12.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 3.7, TEXT);
                ui.label(
                    RichText::new("codex")
                        .font(FontId::monospace(12.0))
                        .strong(),
                );
                ui.label(
                    RichText::new("native")
                        .font(FontId::monospace(10.0))
                        .color(MUTED)
                        .background_color(RAISED),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icons::button(ui, Icon::Sidebar, "Collapse sidebar (Ctrl+B)").clicked() {
                        self.sidebar_collapsed = true;
                    }
                    if icons::button(ui, Icon::Search, "Search (Ctrl+K)").clicked() {
                        self.overlay = Overlay::Search;
                    }
                });
            });
            ui.add_space(10.0);
            let (new_rect, new_response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click());
            ui.painter().rect(
                new_rect,
                CornerRadius::same(11),
                if new_response.hovered() {
                    HOVER
                } else {
                    RAISED
                },
                Stroke::new(1.0, BORDER),
                egui::StrokeKind::Middle,
            );
            ui.painter().text(
                new_rect.left_center() + egui::vec2(12.0, 0.0),
                egui::Align2::LEFT_CENTER,
                "+   New Chat",
                FontId::proportional(12.0),
                TEXT,
            );
            ui.painter().text(
                new_rect.right_center() - egui::vec2(12.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                "Ctrl+N",
                FontId::monospace(10.0),
                MUTED,
            );
            new_response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "New Chat (Ctrl+N)")
            });
            if new_response.clicked() {
                self.new_preview();
            }
            ui.add_space(14.0);

            let remaining = (ui.available_height() - 77.0).max(80.0);
            ScrollArea::vertical()
                .id_salt("sidebar-navigation")
                .max_height(remaining)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(mono("PROJECTS"));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_enabled_ui(false, |ui| {
                                icons::button(
                                    ui,
                                    Icon::Add,
                                    "Project mounting unavailable in preview",
                                )
                            });
                        });
                    });
                    ui.add_space(3.0);
                    for idx in 0..PROJECTS.len() {
                        let selected = !self.standalone && self.project == idx;
                        let choice =
                            ghost_row(ui, PROJECTS[idx], selected, 0.0, Some(self.expanded[idx]));
                        if choice.clicked() {
                            self.expanded[idx] = !self.expanded[idx];
                            if self.engine.is_none() {
                                self.project = idx;
                                self.standalone = false;
                                self.selected_thread = 0;
                            }
                        }
                        if self.expanded[idx] {
                            for (thread, name) in PROJECT_THREADS[idx].iter().enumerate() {
                                let active = !self.standalone
                                    && self.project == idx
                                    && self.selected_thread == thread;
                                if ghost_row(ui, name, active, 18.0, None).clicked()
                                    && self.engine.is_none()
                                {
                                    self.project = idx;
                                    self.selected_thread = thread;
                                    self.standalone = false;
                                }
                            }
                        }
                    }
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        ui.label(mono("CHATS"));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(small("2"));
                        });
                    });
                    for (idx, title) in CHAT_TITLES.iter().enumerate() {
                        if ghost_row(
                            ui,
                            title,
                            self.standalone && self.selected_thread == idx,
                            0.0,
                            None,
                        )
                        .clicked()
                            && self.engine.is_none()
                        {
                            self.standalone = true;
                            self.selected_thread = idx;
                        }
                    }
                });
            ui.separator();
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 50.0), Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(10), HOVER);
            }
            let avatar = egui::Rect::from_min_size(
                rect.left_top() + egui::vec2(5.0, 11.0),
                Vec2::splat(26.0),
            );
            ui.painter().rect(
                avatar,
                CornerRadius::same(7),
                HOVER,
                Stroke::new(1.0, BORDER),
                egui::StrokeKind::Middle,
            );
            ui.painter().text(
                avatar.center(),
                egui::Align2::CENTER_CENTER,
                "dv",
                FontId::monospace(11.0),
                TEXT,
            );
            ui.painter().text(
                rect.left_top() + egui::vec2(40.0, 17.0),
                egui::Align2::LEFT_CENTER,
                "dev@local",
                FontId::proportional(12.0),
                TEXT,
            );
            ui.painter()
                .circle_filled(rect.left_top() + egui::vec2(43.0, 33.0), 3.0, GREEN);
            ui.painter().text(
                rect.left_top() + egui::vec2(51.0, 33.0),
                egui::Align2::LEFT_CENTER,
                "Profile & settings",
                FontId::monospace(10.0),
                MUTED,
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Profile & settings")
            });
            if response.clicked() {
                self.overlay = Overlay::Settings;
            }
        });
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(mono(self.active_project()));
            ui.label(RichText::new("/").color(MUTED));
            ui.label(RichText::new(self.active_title()).color(TEXT).size(12.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new("clear").frame(false)).clicked() {
                    self.new_preview();
                }
                if self.show_reference {
                    ui.label(
                        RichText::new("+28 / -6")
                            .font(FontId::monospace(10.0))
                            .color(GREEN),
                    );
                }
            });
        });
    }

    fn demo_conversation(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        Frame::new()
            .inner_margin(Margin { left: 11, right: 0, top: 0, bottom: 0 })
            .stroke(Stroke { width: 0.0, color: Color32::TRANSPARENT })
            .show(ui, |ui| {
                let top = ui.cursor().top();
                ui.label(mono("you"));
                ui.add_space(4.0);
                ui.label(RichText::new(
                    "Buat parser stream stdout untuk wrapper codex agar chunk JSON tidak terpotong saat di-pipe ke UI."
                ).size(13.0).color(TEXT));
                let bottom = ui.min_rect().bottom();
                ui.painter().line_segment(
                    [egui::pos2(ui.min_rect().left() - 9.0, top + 1.0),
                     egui::pos2(ui.min_rect().left() - 9.0, bottom - 1.0)],
                    Stroke::new(2.0, Color32::from_gray(67)),
                );
            });
        ui.add_space(35.0);
        egui::CollapsingHeader::new(
            RichText::new("Thought for 1.4s")
                .font(FontId::monospace(11.0))
                .color(MUTED),
        )
        .default_open(false)
        .show(ui, |ui| {
            ui.label(body(
                "Reference fixture · inspected stream UTF-8 boundary handling.",
            ));
        });
        ui.add_space(7.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 7.0;
            ui.label(
                RichText::new("codex")
                    .font(FontId::monospace(11.0))
                    .color(TEXT),
            );
            ui.label(small("·"));
            ui.label(mono("read_file(\"src/ipc/stream.ts\") · 48 lines"));
            ui.label(
                RichText::new("+18  -3")
                    .font(FontId::monospace(10.0))
                    .color(GREEN)
                    .background_color(RAISED),
            );
        });
        ui.add_space(11.0);
        ui.label(RichText::new(
            "Gunakan `StringDecoder` dengan sisa buffer baris terakhir agar setiap frame JSON terjamin utuh sebelum di-parse:"
        ).size(13.0).color(SECONDARY));
        ui.add_space(13.0);
        Frame::new()
            .fill(SURFACE)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(CornerRadius::same(11))
            .inner_margin(Margin::symmetric(13, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    ui.label(mono("patch preview"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(egui::Button::new("copy").frame(false)).clicked() {
                            ui.ctx().copy_text(REFERENCE_PATCH.to_owned());
                        }
                        ui.add_enabled(false, egui::Button::new("apply").frame(false))
                            .on_hover_text("No edits can be applied from this read-only prototype");
                    });
                });
                ui.add_space(3.0);
                ui.separator();
                ui.add_space(10.0);
                ScrollArea::horizontal()
                    .id_salt("reference-ts")
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        // The HTML reference uses text-xs/leading-relaxed;
                        // its line boxes are taller than egui's default.
                        for line in REFERENCE_PATCH.lines() {
                            ui.allocate_ui_with_layout(
                                Vec2::new(ui.available_width().max(400.0), 18.5),
                                Layout::left_to_right(Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(if line.is_empty() { " " } else { line })
                                            .font(FontId::monospace(11.5))
                                            .color(TEXT),
                                    );
                                },
                            );
                        }
                    });
                ui.add_space(6.0);
            });
    }

    fn empty_conversation(&mut self, ui: &mut egui::Ui) {
        ui.add_space(78.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(">_")
                    .font(FontId::monospace(21.0))
                    .color(SECONDARY),
            );
            ui.add_space(15.0);
            ui.label(
                RichText::new(format!("Ready in {}", self.active_project()))
                    .color(TEXT)
                    .strong(),
            );
            ui.add_space(4.0);
            ui.label(body(
                "Minimal native shell · official Codex connection is read-only.",
            ));
            ui.add_space(18.0);
            ui.horizontal_wrapped(|ui| {
                for prompt in [
                    "Audit IPC stream parser",
                    "Summarize git diff",
                    "Optimize startup latency",
                ] {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(prompt).size(11.0).color(SECONDARY))
                                .fill(SURFACE),
                        )
                        .clicked()
                    {
                        self.draft = prompt.to_owned();
                    }
                }
            });
        });
    }

    fn conversation(&mut self, ui: &mut egui::Ui) {
        if self.history.is_empty() {
            ScrollArea::vertical()
                .id_salt("empty-or-reference")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    centered_width(ui, 768.0, |ui| {
                        if self.show_reference {
                            self.demo_conversation(ui);
                        } else {
                            self.empty_conversation(ui);
                        }
                    });
                });
            return;
        }
        // Only visible rows are constructed/painter-laid-out: 10,000 rows
        // must not allocate 10,000 epaint objects per frame.
        ScrollArea::vertical()
            .id_salt("native-fixture-chat")
            .auto_shrink([false, false])
            .show_rows(ui, 49.0, self.history.len(), |ui, rows| {
                centered_width(ui, 768.0, |ui| {
                    for i in rows {
                        let row = &self.history[i];
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if row.from_user { "you" } else { "codex" })
                                    .font(FontId::monospace(11.0))
                                    .color(if row.from_user { TEXT } else { SECONDARY }),
                            );
                            ui.label(RichText::new(&row.preview).size(12.0).color(SECONDARY));
                        });
                        ui.add_space(14.0);
                    }
                });
            });
    }

    fn composer(&mut self, ui: &mut egui::Ui) {
        // Reference: max-w-3xl (768px) composer wrapper with px-4 (16px)
        // applied on both sides. The visual input box is therefore 736px.
        let content_w = (ui.available_width().min(768.0) - 32.0).max(280.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(content_w);
            ui.horizontal(|ui| {
                ui.menu_button(
                    RichText::new(format!("▱  {}", self.active_project()))
                        .font(FontId::monospace(11.0))
                        .color(SECONDARY),
                    |ui| {
                        ui.label(small("PROJECT CONTEXT · visual preview"));
                        for name in PROJECTS {
                            ui.add_enabled(false, egui::Button::new(name).frame(false));
                        }
                        ui.separator();
                        ui.label(small("Switching the running engine is not implemented."));
                    },
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(mono(if self.engine.is_some() {
                        "official app-server · read-only"
                    } else {
                        "~/workspace/codex-wrapper-ui · main*"
                    }));
                });
            });
            ui.add_space(4.0);
            theme::compose().show(ui, |ui| {
                ui.add_sized(
                    [ui.available_width(), 48.0],
                    TextEdit::multiline(&mut self.draft)
                        .frame(false)
                        .hint_text("Ask Codex to inspect files, apply a patch, or run a command...")
                        .desired_rows(2)
                        .char_limit(MAX_DRAFT_CHARS),
                );
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    ui.menu_button(
                        RichText::new("●  Read-Only").color(SECONDARY).size(11.0),
                        |ui| {
                            ui.set_min_width(222.0);
                            ui.label(mono("SANDBOX PERMISSION"));
                            ui.separator();
                            for (name, detail) in [
                                ("Full Access", "Read, write files & execute shell"),
                                ("Auto-Edit", "Apply patches, ask before shell"),
                                ("Read-Only", "Inspect without mutations"),
                            ] {
                                ui.add_enabled_ui(false, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("●").size(11.0).color(
                                            if name == "Read-Only" {
                                                SECONDARY
                                            } else {
                                                MUTED
                                            },
                                        ));
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(name).size(11.5).color(TEXT));
                                            ui.label(small(detail));
                                        });
                                    });
                                });
                                ui.add_space(4.0);
                            }
                            ui.separator();
                            ui.label(small("Reference preview. Permissions stay read-only."));
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_enabled_ui(false, |ui| {
                            icons::button(ui, Icon::Send, "Send unavailable: no model turn API")
                        });
                        ui.add_enabled_ui(false, |ui| {
                            icons::button(ui, Icon::Mic, "Dictation not implemented")
                        });
                        ui.menu_button(
                            RichText::new(if self.engine.is_some() {
                                "▂▅  Codex · default  ⌄"
                            } else {
                                "▂▅  o4-mini · high  ⌄"
                            })
                            .font(FontId::monospace(11.0))
                            .color(SECONDARY),
                            |ui| {
                                ui.set_min_width(265.0);
                                ui.horizontal(|ui| {
                                    ui.label(mono("1  SELECT MODEL"));
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(small("codex-core"));
                                    });
                                });
                                for (name, subtitle) in [
                                    ("o4-mini", "Fast reasoning & low latency"),
                                    ("o3", "Deep multi-file synthesis"),
                                    ("gpt-4.1", "Direct instruction following"),
                                ] {
                                    ui.add_enabled_ui(false, |ui| {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(name)
                                                    .font(FontId::monospace(11.0))
                                                    .color(TEXT),
                                            );
                                            ui.label(small(subtitle));
                                        });
                                    });
                                    ui.add_space(4.0);
                                }
                                ui.separator();
                                ui.horizontal(|ui| {
                                    ui.label(mono("2  REASONING EFFORT"));
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(small("preview only"));
                                    });
                                });
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    for option in ["low", "med", "high", "max"] {
                                        ui.add_enabled(
                                            false,
                                            egui::Button::new(
                                                RichText::new(option).font(FontId::monospace(10.0)),
                                            )
                                            .fill(if option == "high" { HOVER } else { SURFACE }),
                                        );
                                    }
                                });
                                ui.add_space(5.0);
                                ui.label(small(
                                    "Visual reference only; model and effort cannot be changed.",
                                ));
                            },
                        );
                    });
                });
            });
        });
        ui.add_space(4.0);
    }

    fn overlays(&mut self, ctx: &egui::Context) {
        if self.overlay == Overlay::Search {
            let mut show = true;
            egui::Window::new("Search & commands")
                .open(&mut show)
                .default_width(400.0)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.search)
                            .hint_text("Jump to a project or demo thread..."),
                    );
                    ui.separator();
                    for (p, names) in PROJECT_THREADS.iter().enumerate() {
                        if self.engine.is_some() {
                            break;
                        }
                        for (t, title) in names.iter().enumerate() {
                            if !title.to_lowercase().contains(&self.search.to_lowercase())
                                && !PROJECTS[p].contains(&self.search.to_lowercase())
                            {
                                continue;
                            }
                            if ui
                                .add(
                                    egui::Button::new(format!("{}    /    {}", PROJECTS[p], title))
                                        .frame(false),
                                )
                                .clicked()
                            {
                                self.project = p;
                                self.selected_thread = t;
                                self.expanded[p] = true;
                                self.standalone = false;
                                self.overlay = Overlay::None;
                            }
                        }
                    }
                    if self.engine.is_some() {
                        ui.label(small(
                            "Search is preview-only while a Codex engine is attached.",
                        ));
                    }
                });
            if !show {
                self.overlay = Overlay::None;
            }
        } else if self.overlay == Overlay::Settings {
            let mut show = true;
            egui::Window::new("Profile & wrapper settings")
                .open(&mut show)
                .default_width(400.0)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(RichText::new("Local native runtime").strong());
                    ui.label(body(self.engine_state()));
                    ui.separator();
                    ui.label(mono("CODEX ENGINE"));
                    ui.label(body(if self.engine.is_some() {
                        "Trusted executable · connected"
                    } else {
                        "No engine configured"
                    }));
                    ui.add_space(8.0);
                    ui.label(mono("CREDENTIALS"));
                    ui.label(small(
                        "Managed by official Codex. This preview never reads or displays secrets.",
                    ));
                    ui.add_space(8.0);
                    ui.label(mono("UI PERFORMANCE"));
                    ui.label(body(
                        "Offline native renderer · no WebView2 · no motion loops",
                    ));
                    if ui.button("Done").clicked() {
                        self.overlay = Overlay::None;
                    }
                });
            if !show {
                self.overlay = Overlay::None;
            }
        }
    }
}

impl eframe::App for RendererSpike {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.shortcuts(ctx);
        // Safe stable state marker for the Windows benchmark harness.
        let title = format!("Ultima Native Renderer Spike | {}", self.engine_state());
        if self.window_title != title {
            self.window_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }

        let sidebar_width = if self.sidebar_collapsed { 70.0 } else { 270.0 };
        egui::SidePanel::left("ultima-sidebar")
            .exact_width(sidebar_width)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(CANVAS)
                    .inner_margin(Margin::symmetric(10, 10)),
            )
            .show(ctx, |ui| self.sidebar(ui));

        egui::CentralPanel::default()
            .frame(Frame::new().fill(CANVAS).inner_margin(Margin {
                left: 10,
                right: 18,
                top: 14,
                bottom: 10,
            }))
            .show(ctx, |ui| {
                egui::TopBottomPanel::top("minimal-header")
                    .frame(
                        Frame::new()
                            .fill(CANVAS)
                            .inner_margin(Margin::symmetric(2, 6)),
                    )
                    .show_inside(ui, |ui| self.header(ui));
                egui::TopBottomPanel::bottom("minimal-composer")
                    .frame(
                        Frame::new()
                            .fill(CANVAS)
                            .inner_margin(Margin::symmetric(3, 3)),
                    )
                    .show_inside(ui, |ui| self.composer(ui));
                self.conversation(ui);
            });
        self.overlays(ctx);
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let history_rows = args
        .iter()
        .find_map(|arg| {
            arg.strip_prefix("--fixture-history=")
                .and_then(|n| n.parse::<usize>().ok())
        })
        .unwrap_or(0)
        .min(MAX_HISTORY_ROWS);
    let show_reference = !args.iter().any(|arg| arg == "--fixture-empty") && history_rows == 0;
    let exe = args
        .iter()
        .find_map(|a| a.strip_prefix("--codex-exe=").map(PathBuf::from));
    let project = args
        .iter()
        .find_map(|a| a.strip_prefix("--project=").map(PathBuf::from));
    if exe.is_some() != project.is_some() {
        eprintln!("Supply both --codex-exe=<absolute path> and --project=<absolute directory>.");
        std::process::exit(2);
    }
    let project_label = project
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|name| name.to_string_lossy().into_owned());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Ultima · Native Codex Preview")
            .with_inner_size([1200.0, 820.0])
            .with_min_inner_size([760.0, 530.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Ultima Native Renderer Spike",
        options,
        Box::new(move |cc| {
            theme::use_system_fonts(&cc.egui_ctx);
            theme::configure(&cc.egui_ctx);
            let engine = exe
                .clone()
                .zip(project.clone())
                .map(|(exe, project)| EngineProbe::start(exe, project, cc.egui_ctx.clone()));
            Ok(Box::new(RendererSpike::new(
                history_rows,
                show_reference,
                engine,
                project_label.clone(),
            )))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_history_and_default_preview_are_separate() {
        let large = RendererSpike::new(MAX_HISTORY_ROWS + 5, false, None, None);
        assert_eq!(large.history.len(), MAX_HISTORY_ROWS);
        assert!(!large.show_reference);
        let preview = RendererSpike::new(0, true, None, None);
        assert!(preview.show_reference);
        assert_eq!(preview.engine_state(), "RENDERER ONLY");
    }

    #[test]
    fn local_new_preview_never_starts_an_agent_turn() {
        let mut app = RendererSpike::new(0, true, None, None);
        app.draft = "Sensitive local draft".into();
        app.new_preview();
        assert!(app.draft.is_empty());
        assert!(!app.show_reference);
        assert!(app.engine.is_none());
    }
}
