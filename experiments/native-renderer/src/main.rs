#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Native-only, offline visual implementation of the supplied Codex wrapper UI.
//! This prototype does not issue turns, execute tools, or grant permissions.
mod engine_probe;
mod icons;
mod theme;

use eframe::egui::{
    self, Align, CornerRadius, FontId, Frame, Key, Layout, Margin, RichText, ScrollArea, Stroke,
    TextEdit,
};
use engine_probe::{EngineProbe, ProbeStatus};
use icons::Icon;
use std::path::PathBuf;
use theme::{body, mono, small, BORDER, CANVAS, GREEN, MUTED, RAISED, SECONDARY, SURFACE, TEXT};

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
        let width = if self.sidebar_collapsed { 43.0 } else { 238.0 };
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
                ui.label(small("native"));
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
            let new_chat = egui::Button::new(
                RichText::new("+    New Chat                                        Ctrl+N")
                    .color(TEXT)
                    .size(11.0),
            )
            .fill(RAISED)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(CornerRadius::same(10));
            if ui
                .add_sized([ui.available_width(), 31.0], new_chat)
                .clicked()
            {
                self.new_preview();
            }
            ui.add_space(12.0);

            let remaining = (ui.available_height() - 73.0).max(80.0);
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
                        ui.horizontal(|ui| {
                            let mark = if self.expanded[idx] { "v" } else { ">" };
                            if ui.add(egui::Button::new(mark).frame(false)).clicked() {
                                self.expanded[idx] = !self.expanded[idx];
                            }
                            let selected = !self.standalone && self.project == idx;
                            let label = egui::Button::new(
                                RichText::new(PROJECTS[idx]).size(11.5).color(if selected {
                                    TEXT
                                } else {
                                    SECONDARY
                                }),
                            )
                            .frame(false);
                            let choice = ui.add_enabled(self.engine.is_none(), label);
                            if choice.clicked() {
                                self.project = idx;
                                self.standalone = false;
                                self.selected_thread = 0;
                                self.expanded[idx] = true;
                            }
                        });
                        if self.expanded[idx] {
                            ui.indent(("project-thread", idx), |ui| {
                                for (thread, name) in PROJECT_THREADS[idx].iter().enumerate() {
                                    let selected = !self.standalone
                                        && self.project == idx
                                        && self.selected_thread == thread;
                                    let text = RichText::new(*name).size(11.0).color(if selected {
                                        TEXT
                                    } else {
                                        MUTED
                                    });
                                    if ui
                                        .add_enabled(
                                            self.engine.is_none(),
                                            egui::Button::new(text).frame(false),
                                        )
                                        .clicked()
                                    {
                                        self.project = idx;
                                        self.selected_thread = thread;
                                        self.standalone = false;
                                    }
                                }
                            });
                        }
                    }
                    ui.add_space(19.0);
                    ui.horizontal(|ui| {
                        ui.label(mono("CHATS"));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(small("2"));
                        });
                    });
                    for (idx, title) in CHAT_TITLES.iter().enumerate() {
                        if ui
                            .add_enabled(
                                self.engine.is_none(),
                                egui::Button::new(
                                    RichText::new(*title).size(11.0).color(SECONDARY),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            self.standalone = true;
                            self.selected_thread = idx;
                        }
                    }
                });
            ui.separator();
            if ui
                .add_sized(
                    [ui.available_width(), 33.0],
                    egui::Button::new(
                        RichText::new("dev@local     ·     Settings")
                            .size(11.0)
                            .color(SECONDARY),
                    )
                    .frame(false),
                )
                .clicked()
            {
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
                        RichText::new("+18 / -3")
                            .font(FontId::monospace(10.0))
                            .color(GREEN),
                    );
                }
            });
        });
    }

    fn demo_conversation(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.separator();
            ui.label(
                RichText::new("you")
                    .font(FontId::monospace(11.0))
                    .color(MUTED),
            );
        });
        ui.label(RichText::new(
            "Buat parser stream stdout untuk wrapper Codex agar chunk JSON tidak terpotong saat di-pipe ke UI."
        ).color(TEXT).size(13.0));
        ui.add_space(30.0);
        egui::CollapsingHeader::new(
            RichText::new("Thought for 1.4s · inspect JSONL boundaries")
                .font(FontId::monospace(10.5))
                .color(MUTED),
        )
        .default_open(false)
        .show(ui, |ui| {
            ui.label(body(
                "Reference fixture only. No private reasoning or live model output.",
            ));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(mono("codex"));
            ui.label(small("·"));
            ui.label(mono("read_file(\"src/ipc/stream.ts\")"));
            ui.label(
                RichText::new("+18  -3")
                    .font(FontId::monospace(10.0))
                    .color(GREEN),
            );
        });
        ui.add_space(5.0);
        ui.label(body(
            "Gunakan buffer untuk menyimpan sisa frame JSONL sebelum mengurai setiap baris:",
        ));
        ui.add_space(9.0);
        Frame::new()
            .fill(SURFACE)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(CornerRadius::same(11))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(mono("patch preview · reference only"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(egui::Button::new("copy").frame(false)).clicked() {
                            ui.ctx().copy_text(
                                "fn push_chunk(buffer: &mut String, chunk: &str) { buffer.push_str(chunk); }".to_owned()
                            );
                        }
                        ui.add_enabled(false, egui::Button::new("apply").frame(false))
                            .on_hover_text("No edits can be applied from this read-only prototype");
                    });
                });
                ui.separator();
                ui.add(egui::Label::new(RichText::new(
                    "fn push_chunk(buffer: &mut String, chunk: &str) {\n    buffer.push_str(chunk);\n    // Decode complete JSONL frames only.\n}"
                ).font(FontId::monospace(12.0)).color(TEXT)).selectable(true));
            });
        ui.add_space(10.0);
        ui.label(small(
            "Illustrative content from the supplied design — not a real Codex response.",
        ));
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
                    ui.set_max_width(745.0);
                    if self.show_reference {
                        self.demo_conversation(ui);
                    } else {
                        self.empty_conversation(ui);
                    }
                });
            return;
        }
        // Only visible rows are constructed/painter-laid-out: 10,000 rows
        // must not allocate 10,000 epaint objects per frame.
        ScrollArea::vertical()
            .id_salt("native-fixture-chat")
            .auto_shrink([false, false])
            .show_rows(ui, 49.0, self.history.len(), |ui, rows| {
                ui.set_max_width(745.0);
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
    }

    fn composer(&mut self, ui: &mut egui::Ui) {
        let content_w = ui.available_width().min(745.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(content_w);
            ui.horizontal(|ui| {
                ui.menu_button(
                    RichText::new(format!(">  {}", self.active_project()))
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
                        "official app-server  ·  read-only"
                    } else {
                        "local preview  ·  no engine"
                    }));
                });
            });
            ui.add_space(4.0);
            theme::compose().show(ui, |ui| {
                ui.add_sized(
                    [ui.available_width(), 52.0],
                    TextEdit::multiline(&mut self.draft)
                        .frame(false)
                        .hint_text("Ask Codex to inspect files, apply a patch, or run a command...")
                        .desired_rows(2)
                        .char_limit(MAX_DRAFT_CHARS),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.menu_button(
                        RichText::new("●  Read-Only").color(SECONDARY).size(11.0),
                        |ui| {
                            ui.label(RichText::new("Sandbox permission").color(MUTED).size(10.0));
                            ui.label(body("Read-Only · native prototype"));
                            ui.add_enabled(false, egui::Button::new("Auto-Edit"));
                            ui.add_enabled(false, egui::Button::new("Full Access"));
                            ui.label(small("Permissions cannot be escalated in this UI."));
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
                            RichText::new("▂▅  Codex  ·  engine default  ⌄")
                                .font(FontId::monospace(11.0))
                                .color(SECONDARY),
                            |ui| {
                                ui.label(mono("1  SELECT MODEL"));
                                ui.label(body("Managed by official Codex App Server"));
                                ui.add_space(8.0);
                                ui.label(mono("2  REASONING EFFORT"));
                                ui.label(small(
                                    "Read-only probe does not query model capabilities.",
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

        let sidebar_width = if self.sidebar_collapsed { 70.0 } else { 266.0 };
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
