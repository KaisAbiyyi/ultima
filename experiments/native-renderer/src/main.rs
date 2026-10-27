#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Native-only, offline visual implementation of the supplied Codex wrapper UI.
//! This prototype does not issue turns, execute tools, or grant permissions.
mod engine_probe;
mod icons;
mod latency;
mod native_tools;
mod theme;

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Frame, Key, Layout, Margin, RichText, ScrollArea,
    Sense, Stroke, TextEdit, Vec2,
};
use engine_probe::{EngineProbe, ProbeEvent, ProbeStatus};
use icons::Icon;
use latency::{Kind, LatencyRecorder};
use native_tools::ToolEvidence;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use theme::{
    body, mono, small, BORDER, CANVAS, GREEN, HOVER, MUTED, RAISED, SECONDARY, SURFACE, TEXT,
};
use ultima_protocol_bridge::{Approval, ApprovalKind, RpcId};

const MAX_HISTORY_ROWS: usize = 10_000;
const MAX_DRAFT_CHARS: usize = 8 * 1024;
const MAX_LIVE_ROWS: usize = 512;
const MAX_LIVE_TEXT_BYTES: usize = 32 * 1024;
const PROJECTS: [&str; 2] = ["codex-wrapper-ui", "pty-sandbox-daemon"];
const PROJECT_THREADS: [[&str; 2]; 2] = [
    [
        "Zero-copy IPC line framing",
        "Minimal dark monochrome tokens",
    ],
    ["macOS seatbelt read-only profile", ""],
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
            Color32::from_white_alpha(if selected {
                if disclosure.is_some() {
                    8
                } else {
                    14
                }
            } else {
                7
            }),
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
    if disclosure.is_some() {
        ui.painter().text(
            rect.right_center() - egui::vec2(9.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            if label == PROJECTS[0] { "2" } else { "1" },
            FontId::monospace(10.0),
            MUTED,
        );
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
    expanded: [bool; 2],
    standalone: bool,
    sidebar_collapsed: bool,
    overlay: Overlay,
    search: String,
    draft: String,
    history: Vec<ChatRow>,
    live_rows: Vec<ChatRow>,
    tool_rows: Vec<ToolEvidence>,
    pending_approvals: Vec<Approval>,
    reviewed_approval: Option<RpcId>,
    coding_enabled: bool,
    active_turn: Option<(String, String)>,
    awaiting_turn_start: bool,
    live_error: Option<String>,
    last_turn_status: Option<String>,
    show_reference: bool,
    engine: Option<EngineProbe>,
    connected_project: Option<String>,
    window_title: String,
    latency: LatencyRecorder,
    stream_paint_pending: Vec<(Instant, u32)>,
    raw_key_started: Option<Instant>,
    keyboard_paint_pending: Option<(Instant, bool)>,
    startup_paint_scheduled: bool,
}

impl RendererSpike {
    #[cfg(test)]
    fn new(
        history_rows: usize,
        show_reference: bool,
        engine: Option<EngineProbe>,
        project: Option<String>,
    ) -> Self {
        Self::with_latency(
            history_rows,
            show_reference,
            engine,
            project,
            LatencyRecorder::new(Instant::now(), None),
        )
    }

    fn with_latency(
        history_rows: usize,
        show_reference: bool,
        engine: Option<EngineProbe>,
        project: Option<String>,
        latency: LatencyRecorder,
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
            expanded: [true, false],
            standalone: false,
            sidebar_collapsed: false,
            overlay: Overlay::None,
            search: String::new(),
            draft: String::new(),
            history,
            live_rows: Vec::new(),
            tool_rows: Vec::new(),
            pending_approvals: Vec::new(),
            reviewed_approval: None,
            coding_enabled: false,
            active_turn: None,
            awaiting_turn_start: false,
            live_error: None,
            last_turn_status: None,
            show_reference,
            engine,
            connected_project: project,
            window_title: String::new(),
            latency,
            stream_paint_pending: Vec::new(),
            raw_key_started: None,
            keyboard_paint_pending: None,
            startup_paint_scheduled: false,
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
        if self.engine.is_some() {
            return "Official Codex session";
        }
        if self.standalone {
            CHAT_TITLES[self.selected_thread.min(CHAT_TITLES.len() - 1)]
        } else {
            PROJECT_THREADS[self.project][self.selected_thread.min(1)]
        }
    }

    fn new_preview(&mut self) {
        // Local-only interaction; no fake Codex thread or approval is created.
        self.draft.clear();
        if self.engine.is_none() {
            self.show_reference = false;
        }
        self.selected_thread = 0;
    }

    fn push_live_row(&mut self, from_user: bool, preview: String) {
        if self.live_rows.len() == MAX_LIVE_ROWS {
            self.live_rows.remove(0);
        }
        self.live_rows.push(ChatRow { from_user, preview });
    }

    fn apply_probe_event(&mut self, event: ProbeEvent) {
        match event {
            ProbeEvent::ThreadReady { .. } => {
                self.live_error = None;
            }
            ProbeEvent::TurnCancelledBeforeStart => {
                self.pending_approvals.clear();
                self.reviewed_approval = None;
                if self.awaiting_turn_start && self.active_turn.is_none() {
                    self.awaiting_turn_start = false;
                    self.last_turn_status = Some("cancelled before start".to_owned());
                    self.live_error = None;
                    self.push_live_row(false, "Turn cancelled before execution.".to_owned());
                }
            }
            ProbeEvent::TurnStarted { thread_id, turn_id } => {
                self.tool_rows.clear();
                self.pending_approvals.clear();
                self.reviewed_approval = None;
                self.active_turn = Some((thread_id, turn_id));
                self.awaiting_turn_start = false;
                self.last_turn_status = Some("running".to_owned());
                self.push_live_row(false, String::new());
            }
            ProbeEvent::AssistantDelta {
                thread_id,
                turn_id,
                text,
            } => {
                if self.active_turn.as_ref() != Some(&(thread_id, turn_id)) {
                    return;
                }
                if let Some(last) = self.live_rows.last_mut().filter(|row| !row.from_user) {
                    let available = MAX_LIVE_TEXT_BYTES.saturating_sub(last.preview.len());
                    let mut end = text.len().min(available);
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    last.preview.push_str(&text[..end]);
                }
            }
            ProbeEvent::AssistantFinal {
                thread_id,
                turn_id,
                text,
            } => {
                if self.active_turn.as_ref() != Some(&(thread_id, turn_id)) {
                    return;
                }
                if let Some(last) = self.live_rows.last_mut().filter(|row| !row.from_user) {
                    let mut end = text.len().min(MAX_LIVE_TEXT_BYTES);
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    last.preview = text[..end].to_owned();
                }
            }
            ProbeEvent::TurnCompleted {
                thread_id,
                turn_id,
                status,
            } => {
                // TurnLifecycle may verify a completed turn using the
                // turn/start response after turn/completed arrived first.
                // No TurnStarted event exists in that valid ordering.
                let completed_before_started =
                    self.awaiting_turn_start && self.active_turn.is_none();
                if !completed_before_started
                    && self.active_turn.as_ref() != Some(&(thread_id, turn_id))
                {
                    return;
                }
                if completed_before_started {
                    self.push_live_row(false, String::new());
                }
                self.active_turn = None;
                self.awaiting_turn_start = false;
                self.pending_approvals.clear();
                self.reviewed_approval = None;
                self.last_turn_status = Some(status.clone());
                if let Some(last) = self.live_rows.last_mut().filter(|row| !row.from_user) {
                    if last.preview.is_empty() {
                        last.preview = format!("Turn completed: {status} (no assistant text)");
                    }
                }
                if status != "completed" && self.live_error.is_none() {
                    self.live_error = Some(format!("Codex turn ended: {status}"));
                }
            }
            ProbeEvent::ToolSnapshot(item) => {
                if self.active_turn.as_ref() != Some(&(item.thread.clone(), item.turn.clone())) {
                    return;
                }
                if let Some(index) = self.tool_rows.iter().position(|old| {
                    old.thread == item.thread && old.turn == item.turn && old.id == item.id
                }) {
                    if self.tool_rows[index] != item {
                        self.reviewed_approval = None;
                    }
                    self.tool_rows.remove(index);
                }
                self.tool_rows.push(item);
                if self.tool_rows.len() > native_tools::MAX_ITEMS {
                    self.tool_rows.remove(0);
                }
            }
            ProbeEvent::ApprovalPending(request) => {
                if self.active_turn.as_ref()
                    != Some(&(request.thread_id.clone(), request.turn_id.clone()))
                {
                    return;
                }
                if !self.pending_approvals.iter().any(|a| a.id == request.id) {
                    self.pending_approvals.push(request);
                }
            }
            ProbeEvent::ApprovalResolved(id) => {
                self.pending_approvals.retain(|a| a.id != id);
                self.reviewed_approval = None;
            }
            ProbeEvent::Error(message) => {
                // A failed turn emits a sanitized Error *before* its final
                // TurnCompleted event. Do not discard the active turn here,
                // otherwise the exact-ID terminal event would be ignored.
                let engine_failed = self.engine.as_ref().is_some_and(|engine| {
                    matches!(engine.status(), ProbeStatus::Failed | ProbeStatus::Stopped)
                });
                if self.active_turn.is_none() || engine_failed {
                    self.awaiting_turn_start = false;
                    self.active_turn = None;
                    if !matches!(
                        self.last_turn_status.as_deref(),
                        Some("completed" | "interrupted" | "failed")
                    ) {
                        self.last_turn_status = Some("error".to_owned());
                    }
                }
                self.live_error = Some(message);
                self.pending_approvals.clear();
                self.reviewed_approval = None;
            }
        }
    }

    fn changes_visible_stream(&self, event: &ProbeEvent) -> bool {
        let Some(row) = self.live_rows.last().filter(|row| !row.from_user) else {
            return false;
        };
        match event {
            ProbeEvent::AssistantDelta {
                thread_id,
                turn_id,
                text,
            } => {
                self.active_turn.as_ref() == Some(&(thread_id.clone(), turn_id.clone()))
                    && !text.is_empty()
                    && row.preview.len() < MAX_LIVE_TEXT_BYTES
            }
            ProbeEvent::AssistantFinal {
                thread_id,
                turn_id,
                text,
            } => {
                let mut end = text.len().min(MAX_LIVE_TEXT_BYTES);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                self.active_turn.as_ref() == Some(&(thread_id.clone(), turn_id.clone()))
                    && row.preview != text[..end]
            }
            _ => false,
        }
    }

    fn consume_engine_events(&mut self) {
        let events = self
            .engine
            .as_ref()
            .map(EngineProbe::take_timed_events)
            .unwrap_or_default();
        for timed in events {
            let visible_stream = self.changes_visible_stream(&timed.event);
            self.apply_probe_event(timed.event);
            if visible_stream {
                // One conservative oldest-event sample per coalesced batch.
                // The transport queue already caps this at 128 items.
                self.stream_paint_pending
                    .push((timed.received_at, timed.coalesced));
            }
        }
    }

    fn send_live_prompt(&mut self) {
        let prompt = self.draft.trim().to_owned();
        if prompt.is_empty() || self.awaiting_turn_start || self.active_turn.is_some() {
            return;
        }
        let Some(engine) = self.engine.as_ref() else {
            return;
        };
        match engine.submit_prompt(prompt.clone()) {
            Ok(()) => {
                self.push_live_row(true, prompt);
                self.awaiting_turn_start = true;
                self.last_turn_status = Some("starting".to_owned());
                self.live_error = None;
                self.show_reference = false;
                self.draft.clear();
            }
            Err(error) => self.live_error = Some(error.to_owned()),
        }
    }

    fn decide_current(&mut self, accept: bool) {
        let Some(request) = self.pending_approvals.first().cloned() else {
            return;
        };
        // Remove before enqueue: no double-click replay if native command ACK is lost.
        self.pending_approvals.remove(0);
        let reviewed = self.reviewed_approval.take() == Some(request.id.clone());
        if accept && !reviewed {
            self.live_error = Some("Review the exact action before allowing it.".to_owned());
            return;
        }
        if let Some(engine) = self.engine.as_ref() {
            if let Err(reason) = engine.decide(request, accept) {
                self.live_error = Some(reason.to_owned());
            }
        }
    }

    fn approval_card(&mut self, ui: &mut egui::Ui) {
        let Some(request) = self.pending_approvals.first().cloned() else {
            return;
        };
        let evidence = self
            .tool_rows
            .iter()
            .rev()
            .find(|item| {
                item.thread == request.thread_id
                    && item.turn == request.turn_id
                    && item.id == request.item_id
                    && item.kind == request.kind
            })
            .cloned();
        Frame::new()
            .fill(RAISED)
            .stroke(Stroke::new(1.0, BORDER))
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("OFFICIAL TOOL APPROVAL · ALLOW ONCE")
                        .color(TEXT)
                        .strong(),
                );
                ui.label(mono(format!(
                    "request: {:?} · turn: {}",
                    request.id, request.turn_id
                )));
                let preview = evidence.as_ref().map(|item| {
                    if item.kind == ApprovalKind::Command {
                        item.command.as_str()
                    } else {
                        item.diff.as_str()
                    }
                });
                if ui.button("Review exact command / diff").clicked() {
                    if evidence
                        .as_ref()
                        .is_some_and(|item| item.complete && !item.truncated)
                    {
                        self.reviewed_approval = Some(request.id.clone());
                    }
                }
                if self.reviewed_approval.as_ref() == Some(&request.id) {
                    ScrollArea::both().max_height(175.0).show(ui, |ui| {
                        ui.label(
                            RichText::new(preview.unwrap_or("Missing official preview."))
                                .monospace(),
                        );
                    });
                } else {
                    ui.label(small("Unreviewed request. Never approved automatically."));
                }
                ui.horizontal(|ui| {
                    if ui.button("Decline").clicked() {
                        self.decide_current(false);
                    }
                    if ui
                        .add_enabled(
                            self.reviewed_approval.as_ref() == Some(&request.id)
                                && evidence
                                    .as_ref()
                                    .is_some_and(|item| item.complete && !item.truncated),
                            egui::Button::new("Allow once"),
                        )
                        .clicked()
                    {
                        self.decide_current(true);
                    }
                });
            });
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
            ui.add_space(4.0);

            let remaining = (ui.available_height() - 63.0).max(80.0);
            ScrollArea::vertical()
                .id_salt("sidebar-navigation")
                .max_height(remaining)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(mono("PROJECTS"));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_enabled(false, egui::Button::new("+").frame(false))
                                .on_hover_text("Project mounting unavailable in preview");
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
                                if name.is_empty() {
                                    continue;
                                }
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
                    ui.add_space(4.0);
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
            ui.label(
                RichText::new(self.active_title())
                    .color(TEXT)
                    .size(12.0)
                    .strong(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new("clear").frame(false)).clicked() {
                    self.new_preview();
                }
                if self.show_reference {
                    Frame::new()
                        .fill(RAISED)
                        .stroke(Stroke::new(1.0, BORDER))
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(Margin::symmetric(6, 3))
                        .show(ui, |ui| {
                            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                ui.label(
                                    RichText::new("+28")
                                        .font(FontId::monospace(10.0))
                                        .color(GREEN),
                                );
                                ui.label(small("/"));
                                ui.label(
                                    RichText::new("-6")
                                        .font(FontId::monospace(10.0))
                                        .color(Color32::from_rgb(251, 113, 133)),
                                );
                            });
                        });
                }
            });
        });
    }

    fn demo_conversation(&mut self, ui: &mut egui::Ui) {
        ui.add_space(9.0);
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
        ui.add_space(20.0);
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
        ui.add_space(5.0);
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
        ui.add_space(7.0);
        ui.label(RichText::new(
            "Gunakan `StringDecoder` dengan sisa buffer baris terakhir agar setiap frame JSON terjamin utuh sebelum di-parse:"
        ).size(13.0).color(SECONDARY));
        ui.add_space(6.0);
        Frame::new()
            .fill(SURFACE)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(CornerRadius::same(11))
            .inner_margin(Margin {
                left: 13,
                right: 13,
                top: 2,
                bottom: 10,
            })
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
                ui.add_space(0.0);
                ui.separator();
                ui.add_space(5.0);
                ScrollArea::horizontal()
                    .id_salt("reference-ts")
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        // The HTML reference uses text-xs/leading-relaxed;
                        // its line boxes are taller than egui's default.
                        for line in REFERENCE_PATCH.lines() {
                            ui.allocate_ui_with_layout(
                                Vec2::new(ui.available_width().max(400.0), 19.3),
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
                "Minimal native shell · official Codex uses a read-only sandbox.",
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
        if self.engine.is_some() && !self.live_rows.is_empty() {
            ScrollArea::vertical()
                .id_salt("native-live-conversation")
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    centered_width(ui, 768.0, |ui| {
                        for row in &self.live_rows {
                            ui.label(mono(if row.from_user { "you" } else { "codex" }));
                            ui.add_space(4.0);
                            ui.label(RichText::new(&row.preview).color(if row.from_user {
                                TEXT
                            } else {
                                SECONDARY
                            }));
                            ui.add_space(17.0);
                        }
                        for item in &self.tool_rows {
                            let description = match item.kind {
                                ApprovalKind::Command => format!("command · {} · {}", item.state, item.command),
                                ApprovalKind::FileChange => format!("file change · {} · {}", item.state, item.paths.join(", ")),
                            };
                            ui.collapsing(RichText::new(description).monospace(), |ui| {
                                if item.truncated { ui.label("Truncated or incomplete tool evidence; approval unavailable."); }
                                if !item.output.is_empty() {
                                    egui::CollapsingHeader::new("Command result").show(ui, |ui| {
                                        ScrollArea::both().max_height(160.0).show(ui, |ui| { ui.monospace(&item.output); });
                                    });
                                }
                                if !item.diff.is_empty() {
                                    egui::CollapsingHeader::new("Lazy diff").show(ui, |ui| {
                                        ScrollArea::both().max_height(200.0).show(ui, |ui| { ui.monospace(&item.diff); });
                                    });
                                }
                            });
                        }
                    });
                });
            return;
        }
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
        let can_send = self.engine.as_ref().is_some_and(|engine| {
            engine.status() == ProbeStatus::Ready
                && !self.awaiting_turn_start
                && self.active_turn.is_none()
                && !self.draft.trim().is_empty()
        });
        let can_cancel = self.active_turn.is_some() || self.awaiting_turn_start;
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
                    ui.label(mono(if self.engine.is_some() && self.coding_enabled {
                        "official app-server · approval-gated coding"
                    } else if self.engine.is_some() {
                        "official app-server · read-only sandbox"
                    } else {
                        "~/workspace/codex-wrapper-ui · main*"
                    }));
                });
            });
            ui.add_space(4.0);
            theme::compose().show(ui, |ui| {
                let editor = ui.add_sized(
                    [ui.available_width(), 48.0],
                    TextEdit::multiline(&mut self.draft)
                        .frame(false)
                        .hint_text("Ask Codex to inspect files, apply a patch, or run a command...")
                        .desired_rows(2)
                        .char_limit(MAX_DRAFT_CHARS),
                );
                if editor.changed() {
                    if let Some(stamp) = self.raw_key_started.take() {
                        self.keyboard_paint_pending
                            .get_or_insert((stamp, self.active_turn.is_some()));
                    }
                }
                ui.add_space(7.0);
                if let Some(error) = &self.live_error {
                    ui.label(RichText::new(error).color(Color32::from_rgb(251, 113, 133)));
                }
                if let Some(status) = &self.last_turn_status {
                    ui.label(mono(format!("Turn status: {status}")));
                }
                self.approval_card(ui);
                ui.horizontal(|ui| {
                    ui.menu_button(
                        RichText::new(if self.coding_enabled {
                            "●  Approval-gated coding"
                        } else {
                            "●  Read-Only"
                        })
                        .color(SECONDARY)
                        .size(11.0),
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
                            ui.label(small(
                                "Mode fixed at launch; permission changes require reconnect.",
                            ));
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_enabled_ui(can_send, |ui| {
                                icons::button(ui, Icon::Send, "Send prompt to official Codex")
                            })
                            .inner
                            .clicked()
                        {
                            self.send_live_prompt();
                        }
                        if can_cancel && ui.button("Stop").clicked() {
                            if let Some(engine) = &self.engine {
                                if let Err(error) = engine.cancel() {
                                    self.live_error = Some(error.to_owned());
                                } else {
                                    self.pending_approvals.clear();
                                    self.reviewed_approval = None;
                                    self.last_turn_status =
                                        Some("cancellation requested".to_owned());
                                }
                            }
                        }
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
                            if title.is_empty() {
                                continue;
                            }
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

    fn schedule_paint_probe(&mut self, ctx: &egui::Context) {
        let stream = std::mem::take(&mut self.stream_paint_pending);
        let keyboard = self.keyboard_paint_pending.take();
        let startup = if self.startup_paint_scheduled {
            None
        } else {
            self.startup_paint_scheduled = true;
            Some(self.latency.started_at())
        };
        if stream.is_empty() && keyboard.is_none() && startup.is_none() {
            return;
        }
        let recorder = self.latency.clone();
        // Some renderers may replay paint callbacks: a sample belongs to one
        // prepared frame and must be counted only once.
        let fired = Arc::new(AtomicBool::new(false));
        let callback = eframe::egui_glow::CallbackFn::new(move |_, _| {
            if fired.swap(true, Ordering::AcqRel) {
                return;
            }
            for (received, coalesced) in &stream {
                recorder.record_paint(Kind::Stream, *received, *coalesced);
            }
            if let Some((started, running_turn)) = keyboard {
                recorder.record_paint(Kind::Keyboard, started, 0);
                if running_turn {
                    recorder.record_paint(Kind::KeyboardDuringTurn, started, 0);
                }
            }
            if let Some(started) = startup {
                recorder.record_paint(Kind::Startup, started, 0);
            }
        });
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("nc10b-gl-paint-timing"),
        ))
        .add(egui::PaintCallback {
            rect: ctx.content_rect(),
            callback: Arc::new(callback),
        });
    }
}

impl eframe::App for RendererSpike {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        if input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Text(_)
                    | egui::Event::Paste(_)
                    | egui::Event::Key { pressed: true, .. }
            )
        }) {
            // Raw input arrives before egui processes TextEdit. Sample only if
            // the actual composer reports a change during this same frame.
            self.raw_key_started = Some(Instant::now());
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.consume_engine_events();
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
        self.schedule_paint_probe(ctx);
        self.raw_key_started = None;
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.latency.save_on_exit();
    }
}

fn main() -> eframe::Result {
    let main_started = Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let latency_receipt = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--latency-receipt=").map(PathBuf::from));
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
    let coding_enabled = args.iter().any(|arg| arg == "--enable-native-coding");
    if coding_enabled && (exe.is_none() || project.is_none()) {
        eprintln!("Native coding requires explicit --codex-exe and --project paths.");
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
            let engine = exe.clone().zip(project.clone()).map(|(exe, project)| {
                EngineProbe::start(exe, project, cc.egui_ctx.clone(), coding_enabled)
            });
            let mut app = RendererSpike::with_latency(
                history_rows,
                show_reference,
                engine,
                project_label.clone(),
                LatencyRecorder::new(main_started, latency_receipt.clone()),
            );
            app.coding_enabled = coding_enabled;
            Ok(Box::new(app))
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

    #[test]
    fn scoped_native_deltas_never_mix_other_turns() {
        let mut app = RendererSpike::new(0, false, None, None);
        app.apply_probe_event(ProbeEvent::TurnStarted {
            thread_id: "thread-1".into(),
            turn_id: "turn-1".into(),
        });
        app.apply_probe_event(ProbeEvent::AssistantDelta {
            thread_id: "thread-2".into(),
            turn_id: "turn-1".into(),
            text: "wrong thread".into(),
        });
        app.apply_probe_event(ProbeEvent::AssistantDelta {
            thread_id: "thread-1".into(),
            turn_id: "turn-2".into(),
            text: "wrong turn".into(),
        });
        assert_eq!(app.live_rows[0].preview, "");
        app.apply_probe_event(ProbeEvent::AssistantDelta {
            thread_id: "thread-1".into(),
            turn_id: "turn-1".into(),
            text: "expected".into(),
        });
        assert_eq!(app.live_rows[0].preview, "expected");
        app.apply_probe_event(ProbeEvent::AssistantFinal {
            thread_id: "thread-1".into(),
            turn_id: "turn-1".into(),
            text: "canonical final".into(),
        });
        assert_eq!(app.live_rows[0].preview, "canonical final");
        app.apply_probe_event(ProbeEvent::TurnCompleted {
            thread_id: "thread-2".into(),
            turn_id: "turn-1".into(),
            status: "completed".into(),
        });
        assert!(app.active_turn.is_some());
    }

    #[test]
    fn native_live_row_and_utf8_buffers_are_bounded() {
        let mut app = RendererSpike::new(0, false, None, None);
        for i in 0..MAX_LIVE_ROWS + 3 {
            app.push_live_row(true, format!("{i}"));
        }
        assert_eq!(app.live_rows.len(), MAX_LIVE_ROWS);
        assert_eq!(app.live_rows[0].preview, "3");
        app.apply_probe_event(ProbeEvent::TurnStarted {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
        });
        for _ in 0..100 {
            app.apply_probe_event(ProbeEvent::AssistantDelta {
                thread_id: "thread".into(),
                turn_id: "turn".into(),
                text: "語".repeat(800),
            });
        }
        let final_text = &app.live_rows.last().unwrap().preview;
        assert!(final_text.len() <= MAX_LIVE_TEXT_BYTES);
        assert!(final_text.chars().all(|c| c == '語'));
    }

    #[test]
    fn native_turn_terminal_status_distinguishes_completion_and_interruption() {
        for status in ["completed", "interrupted", "failed"] {
            let mut app = RendererSpike::new(0, false, None, None);
            app.apply_probe_event(ProbeEvent::TurnStarted {
                thread_id: "thread".into(),
                turn_id: "turn".into(),
            });
            assert_eq!(app.last_turn_status.as_deref(), Some("running"));
            app.apply_probe_event(ProbeEvent::TurnCompleted {
                thread_id: "wrong-thread".into(),
                turn_id: "turn".into(),
                status: "interrupted".into(),
            });
            assert_eq!(app.last_turn_status.as_deref(), Some("running"));
            app.apply_probe_event(ProbeEvent::TurnCompleted {
                thread_id: "thread".into(),
                turn_id: "turn".into(),
                status: status.into(),
            });
            assert_eq!(app.last_turn_status.as_deref(), Some(status));
            assert!(app.active_turn.is_none());
            if status == "completed" {
                assert!(app.live_error.is_none());
            } else {
                assert!(app.live_error.is_some());
            }
        }
    }

    #[test]
    fn failed_turn_error_does_not_hide_authoritative_terminal_event() {
        let mut app = RendererSpike::new(0, false, None, None);
        app.apply_probe_event(ProbeEvent::TurnStarted {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
        });
        app.apply_probe_event(ProbeEvent::Error(
            "Codex reached its upstream usage or rate limit.".into(),
        ));
        assert!(app.active_turn.is_some());
        assert_eq!(app.last_turn_status.as_deref(), Some("running"));
        app.apply_probe_event(ProbeEvent::TurnCompleted {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            status: "failed".into(),
        });
        assert!(app.active_turn.is_none());
        assert_eq!(app.last_turn_status.as_deref(), Some("failed"));
        assert_eq!(
            app.live_error.as_deref(),
            Some("Codex reached its upstream usage or rate limit.")
        );
        // Late cancellation errors cannot rewrite a completed turn.
        app.apply_probe_event(ProbeEvent::Error(
            "There is no active turn to cancel.".into(),
        ));
        assert_eq!(app.last_turn_status.as_deref(), Some("failed"));
    }

    #[test]
    fn verified_terminal_before_start_ack_releases_native_composer() {
        let mut app = RendererSpike::new(0, false, None, None);
        app.awaiting_turn_start = true;
        app.push_live_row(true, "test prompt".into());
        // The protocol layer has matched the early terminal to the official
        // turn/start response, so it can deliver this without TurnStarted.
        app.apply_probe_event(ProbeEvent::TurnCompleted {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            status: "failed".into(),
        });
        app.apply_probe_event(ProbeEvent::Error("Upstream limit reached.".into()));
        assert!(!app.awaiting_turn_start);
        assert!(app.active_turn.is_none());
        assert_eq!(app.last_turn_status.as_deref(), Some("failed"));
        assert_eq!(app.live_error.as_deref(), Some("Upstream limit reached."));
        assert_eq!(app.live_rows.len(), 2);
        assert!(app.live_rows[1].preview.contains("failed"));

        let mut idle = RendererSpike::new(0, false, None, None);
        idle.apply_probe_event(ProbeEvent::TurnCompleted {
            thread_id: "old-thread".into(),
            turn_id: "old-turn".into(),
            status: "completed".into(),
        });
        assert!(idle.live_rows.is_empty(), "unsolicited terminal ignored");
        assert!(idle.last_turn_status.is_none());
    }

    #[test]
    fn cancelled_thread_start_never_leaves_ui_awaiting() {
        let mut app = RendererSpike::new(0, false, None, None);
        app.awaiting_turn_start = true;
        app.push_live_row(true, "cancel this".into());
        app.apply_probe_event(ProbeEvent::TurnCancelledBeforeStart);
        assert!(!app.awaiting_turn_start);
        assert!(app.active_turn.is_none());
        assert_eq!(
            app.last_turn_status.as_deref(),
            Some("cancelled before start")
        );
        assert!(app.live_error.is_none());
        assert_eq!(app.live_rows[1].preview, "Turn cancelled before execution.");
    }

    #[test]
    fn latency_samples_only_authoritative_visible_text_changes() {
        let mut app = RendererSpike::new(0, false, None, None);
        let delta = |thread: &str, text: &str| ProbeEvent::AssistantDelta {
            thread_id: thread.into(),
            turn_id: "turn".into(),
            text: text.into(),
        };
        assert!(!app.changes_visible_stream(&delta("thread", "x")));
        app.apply_probe_event(ProbeEvent::TurnStarted {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
        });
        assert!(!app.changes_visible_stream(&delta("stale-thread", "x")));
        assert!(!app.changes_visible_stream(&delta("thread", "")));
        assert!(app.changes_visible_stream(&delta("thread", "visible")));
        app.apply_probe_event(delta("thread", "visible"));
        let final_event = ProbeEvent::AssistantFinal {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            text: "visible".into(),
        };
        assert!(!app.changes_visible_stream(&final_event));
        assert!(app.changes_visible_stream(&ProbeEvent::AssistantFinal {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            text: "different canonical reply".into(),
        }));
    }
}
