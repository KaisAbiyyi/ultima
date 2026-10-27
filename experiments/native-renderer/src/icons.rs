//! Tiny native vector symbols. No glyph fallback, asset decode or icon font.
use crate::theme::{HOVER, SECONDARY, TEXT};
use eframe::egui::{
    self, Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
};

#[derive(Clone, Copy)]
pub enum Icon {
    Sidebar,
    Search,
    Add,
    Settings,
    Mic,
    Send,
}

pub fn button(ui: &mut Ui, icon: Icon, description: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), description)
    });
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(7), HOVER);
        }
        draw(
            ui,
            rect,
            icon,
            if response.hovered() { TEXT } else { SECONDARY },
        );
    }
    response.on_hover_text(description)
}

fn draw(ui: &Ui, rect: Rect, icon: Icon, color: Color32) {
    let painter = ui.painter();
    let c = rect.center();
    let s = Stroke::new(1.45, color);
    match icon {
        Icon::Sidebar => {
            painter.rect_stroke(
                Rect::from_center_size(c, Vec2::new(15.0, 13.0)),
                CornerRadius::same(2),
                s,
                StrokeKind::Middle,
            );
            painter.line_segment([c + egui::vec2(-2.6, -6.3), c + egui::vec2(-2.6, 6.3)], s);
        }
        Icon::Search => {
            painter.circle_stroke(c + egui::vec2(-1.2, -1.4), 5.1, s);
            painter.line_segment([c + egui::vec2(2.7, 3.0), c + egui::vec2(7.3, 7.3)], s);
        }
        Icon::Add => {
            painter.line_segment([c + egui::vec2(-5.0, 0.0), c + egui::vec2(5.0, 0.0)], s);
            painter.line_segment([c + egui::vec2(0.0, -5.0), c + egui::vec2(0.0, 5.0)], s);
        }
        Icon::Settings => {
            painter.circle_stroke(c, 5.9, s);
            painter.circle_stroke(c, 2.1, s);
            for idx in 0..8 {
                let a = idx as f32 * std::f32::consts::TAU / 8.0;
                let v = egui::vec2(a.cos(), a.sin());
                painter.line_segment([c + v * 6.7, c + v * 8.4], s);
            }
        }
        Icon::Mic => {
            painter.rect_stroke(
                Rect::from_center_size(c + egui::vec2(0.0, -2.0), Vec2::new(5.0, 9.0)),
                CornerRadius::same(3),
                s,
                StrokeKind::Middle,
            );
            let lower_arc: Vec<_> = (0..=12)
                .map(|step| {
                    let a = (step as f32) * std::f32::consts::PI / 12.0;
                    c + egui::vec2(a.cos(), a.sin()) * 6.0
                })
                .collect();
            painter.add(egui::Shape::line(lower_arc, s));
            painter.line_segment([c + egui::vec2(0.0, 6.0), c + egui::vec2(0.0, 8.3)], s);
            painter.line_segment([c + egui::vec2(-3.5, 8.3), c + egui::vec2(3.5, 8.3)], s);
        }
        Icon::Send => {
            painter.line_segment([c + egui::vec2(0.0, 7.0), c + egui::vec2(0.0, -7.0)], s);
            painter.line_segment([c + egui::vec2(-5.3, -1.6), c + egui::vec2(0.0, -7.0)], s);
            painter.line_segment([c + egui::vec2(5.3, -1.6), c + egui::vec2(0.0, -7.0)], s);
        }
    }
}
