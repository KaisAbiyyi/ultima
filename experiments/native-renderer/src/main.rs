#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Isolated NC-10 renderer feasibility spike. Does not launch Codex or accept approvals.
mod engine_probe;

use eframe::egui::{self, Align, Color32, FontId, RichText, ScrollArea, TextEdit};
use engine_probe::{EngineProbe, ProbeStatus};
use std::path::PathBuf;

const MAX_HISTORY_ROWS: usize = 10_000;
const MAX_DRAFT_CHARS: usize = 8 * 1024;
const PROJECTS: [&str; 3] = ["fixture-a", "fixture-b", "fixture-c"];

struct ChatRow {
    from_user: bool,
    preview: String,
}

struct RendererSpike {
    project: usize,
    selected_thread: usize,
    draft: String,
    history: Vec<ChatRow>,
    show_approval: bool,
    mode: &'static str,
    engine: Option<EngineProbe>,
    connected_project: Option<String>,
    window_title: String,
}

impl RendererSpike {
    fn new(history_rows: usize, engine: Option<EngineProbe>, project: Option<String>) -> Self {
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
            draft: String::new(),
            history,
            show_approval: true,
            mode: if history_rows == 0 {
                "empty"
            } else {
                "fixture-history"
            },
            engine,
            connected_project: project,
            window_title: String::new(),
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("ultima.").size(23.0).strong());
        ui.label(RichText::new("CODEX / NATIVE RENDERER").small().weak());
        ui.add_space(18.0);
        ui.add_enabled(false, egui::Button::new("+ New thread (prototype)"));
        ui.add_space(15.0);
        ui.label(RichText::new("PROJECTS").small().weak());
        for (idx, project) in PROJECTS.iter().enumerate() {
            if ui
                .add_enabled(
                    self.engine.is_none(),
                    egui::Button::new(*project).selected(self.project == idx),
                )
                .clicked()
            {
                self.project = idx;
                self.selected_thread = 0;
                self.draft.clear();
            }
        }
        ui.separator();
        ui.label(
            RichText::new("RECENT THREADS (fixture only)")
                .small()
                .weak(),
        );
        for idx in 0..4 {
            if ui
                .selectable_label(
                    self.selected_thread == idx,
                    format!("Fixture thread {}", idx + 1),
                )
                .clicked()
            {
                self.selected_thread = idx;
            }
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new("No model turns • No permissions granted")
                .small()
                .weak(),
        );
    }

    fn conversation(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Workspace");
            ui.label("/");
            ui.strong(
                self.connected_project
                    .as_deref()
                    .unwrap_or(PROJECTS[self.project]),
            );
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let status = match self.engine.as_ref().map(|engine| engine.status()) {
                    Some(ProbeStatus::Connecting) => "APP SERVER CONNECTING",
                    Some(ProbeStatus::Ready) => "APP SERVER READY • READ ONLY",
                    Some(ProbeStatus::Failed) => "APP SERVER ERROR",
                    Some(ProbeStatus::Stopped) => "APP SERVER STOPPED",
                    None => "RENDERER ONLY",
                };
                ui.label(RichText::new(status).color(Color32::from_rgb(160, 166, 196)));
            });
        });
        ui.separator();
        ui.add_space(10.0);
        ui.label(RichText::new("Codex / Local App Server").small().weak());
        ui.heading(RichText::new("Build with clarity.").size(24.0));
        ui.label(format!(
            "{} messages • {} mode",
            self.history.len(),
            self.mode
        ));
        ui.add_space(4.0);
        ui.separator();

        // The same lazy-visible-row principle as the production Svelte client:
        // only the visible chat rows are laid out or painted.
        let reserved = 180.0;
        let feed_height = (ui.available_height() - reserved).max(80.0);
        ScrollArea::vertical()
            .id_salt("native-fixture-chat")
            .max_height(feed_height)
            .auto_shrink([false, false])
            .show_rows(ui, 44.0, self.history.len(), |ui, rows| {
                for i in rows {
                    let row = &self.history[i];
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(if row.from_user { "YOU" } else { "CODEX" })
                                .font(FontId::proportional(12.0))
                                .color(if row.from_user {
                                    Color32::from_rgb(127, 175, 240)
                                } else {
                                    Color32::from_rgb(182, 187, 225)
                                }),
                        );
                        ui.label(&row.preview);
                    });
                    ui.add_space(8.0);
                }
            });
        ui.separator();
        ui.add_space(4.0);
        ui.label(
            RichText::new("Composer • read-only engine probe (no model turn API)")
                .small()
                .weak(),
        );
        ui.add(
            TextEdit::multiline(&mut self.draft)
                .hint_text("Typing is local to this renderer spike")
                .desired_rows(2)
                .char_limit(MAX_DRAFT_CHARS),
        );
        ui.horizontal(|ui| {
            ui.add_enabled(false, egui::Button::new("Send to Codex"));
            ui.label(
                RichText::new("No network · No execution · No approval")
                    .small()
                    .weak(),
            );
        });
        if self.show_approval {
            ui.collapsing("Tool approval mock • always denied", |ui| {
                ui.label("A real tool approval needs a matching thread, turn, and tool preview.");
                ui.add_enabled(false, egui::Button::new("Allow once"));
                if ui.button("Dismiss mock panel").clicked() {
                    self.show_approval = false;
                }
            });
        }
    }
}

impl eframe::App for RendererSpike {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Host-visible, sanitized readiness marker used by the Windows
        // feasibility harness. It carries no thread text or user paths.
        let state = match self.engine.as_ref().map(EngineProbe::status) {
            Some(ProbeStatus::Connecting) => "APP SERVER CONNECTING",
            Some(ProbeStatus::Ready) => "APP SERVER READY",
            Some(ProbeStatus::Failed) => "APP SERVER FAILED",
            Some(ProbeStatus::Stopped) => "APP SERVER STOPPED",
            None => "RENDERER ONLY",
        };
        let title = format!("Ultima Native Renderer Spike | {state}");
        if self.window_title != title {
            self.window_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        egui::SidePanel::left("ultima-sidebar")
            .min_width(210.0)
            .default_width(235.0)
            .resizable(false)
            .show(ctx, |ui| self.sidebar(ui));
        egui::CentralPanel::default().show(ctx, |ui| self.conversation(ui));
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let history_rows = std::env::args()
        .find_map(|arg| {
            arg.strip_prefix("--fixture-history=")
                .and_then(|n| n.parse::<usize>().ok())
        })
        .unwrap_or(0)
        .min(MAX_HISTORY_ROWS);
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
            .with_title("Ultima • Native Renderer Spike")
            .with_inner_size([1200.0, 820.0])
            .with_min_inner_size([760.0, 530.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Ultima Native Renderer Spike",
        options,
        Box::new(move |cc| {
            let engine = exe
                .clone()
                .zip(project.clone())
                .map(|(exe, project)| EngineProbe::start(exe, project, cc.egui_ctx.clone()));
            Ok(Box::new(RendererSpike::new(
                history_rows,
                engine,
                project_label.clone(),
            )))
        }),
    )
}
