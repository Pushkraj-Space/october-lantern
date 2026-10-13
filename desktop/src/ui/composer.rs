//! The chat-only composer. A send snapshots the pinned recipient; it never chooses one at send time.
use super::{
    Ctx, DictationOwner, UiAction,
    components::{self as c},
    icons::Icon,
    theme::{self as t, Weight},
};
use crate::model::Agent;
use eframe::egui::{self, Margin, Sense, Stroke, Ui};

pub fn render(ui: &mut Ui, cx: &mut Ctx, agent: &Agent) {
    egui::Frame::new().inner_margin(Margin { left: 12, right: 12, top: 4, bottom: 12 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        if !agent.can_reply {
            c::label(
                ui,
                format!("Your reply is copied, and Lantern opens {}", agent.host.as_ref().map_or("its terminal", |host| host.app.as_str())),
                11.0,
                Weight::Regular,
                t::MUTED,
            );
        }
        egui::Frame::new()
            .fill(t::FAINT)
            .stroke(Stroke::new(1.0, if cx.model.dictating { t::opacity(t::RED, 0.5) } else { t::STROKE }))
            .corner_radius(12)
            .inner_margin(Margin::symmetric(11, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let width = (ui.available_width() - 46.0).max(50.0);
                    let edit = egui::ScrollArea::vertical()
                        .id_salt(("composer-scroll", &agent.id))
                        .max_height(80.0)
                        .min_scrolled_height(0.0)
                        .show(ui, |ui| {
                            ui.set_width(width);
                            cx.inputs.drafts.edit_text(None, |text| {
                                ui.add(
                                    egui::TextEdit::multiline(text)
                                        .id_salt(("composer", &agent.id))
                                        .font(t::font(13.0, Weight::Regular))
                                        .text_color(t::INK)
                                        .hint_text(c::text(format!("Message @{}…", agent.handle), 13.0, Weight::Regular, t::MUTED))
                                        .frame(egui::Frame::NONE)
                                        .margin(0)
                                        .desired_width(width)
                                        .desired_rows(1)
                                        .return_key(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter)),
                                )
                            })
                        })
                        .inner;
                    let focus = Some((agent.id.clone(), cx.model.compose_focus));
                    if cx.view.composer_focus != focus {
                        edit.request_focus();
                        cx.view.composer_focus = focus;
                    }
                    let submit = edit.has_focus() && c::consume_plain_key(ui, egui::Key::Enter);
                    let mic = if cx.model.dictating { Icon::Waveform } else { Icon::Mic };
                    let (mic_rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 22.0), Sense::hover());
                    c::paint_icon(
                        ui.painter(),
                        egui::Rect::from_center_size(mic_rect.center(), egui::Vec2::splat(12.0)),
                        mic,
                        if cx.model.dictating { t::RED } else { t::MUTED },
                    );
                    let mic_response = ui.interact(mic_rect.expand(2.0), ui.id().with(("composer-mic", &agent.id)), Sense::click());
                    mic_response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            ui.is_enabled(),
                            if cx.model.dictating { "Stop dictation" } else { "Dictate" },
                        )
                    });
                    // These backends open the recorder directly; there is no permission-wait stage.
                    if mic_response.on_hover_text("Dictate").clicked() {
                        cx.emit(UiAction::ToggleDictation { owner: DictationOwner::Reply(agent.id.clone()) });
                    }
                    let enabled =
                        !cx.inputs.drafts.text().is_empty() && !cx.model.sending && cx.inputs.drafts.recipient() == Some(agent.id.as_str());
                    let response = ui
                        .add_enabled_ui(enabled, |ui| {
                            let (rect, response) = ui.allocate_exact_size(egui::vec2(18.0, 22.0), Sense::click());
                            let circle = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(18.0));
                            let color = if enabled { t::AMBER } else { t::MUTED };
                            ui.painter().circle_filled(circle.center(), 9.0, color);
                            c::paint_icon(
                                ui.painter(),
                                circle.shrink(4.0),
                                if cx.model.sending { Icon::Hourglass } else { Icon::ArrowUp },
                                t::black(0.85),
                            );
                            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, "Send"));
                            response
                        })
                        .inner
                        .on_hover_text(if cx.model.sending { "Sending…" } else { "Send" });
                    if enabled && (submit || response.clicked()) {
                        cx.emit(UiAction::SendReply { agent_id: agent.id.clone() });
                    }
                });
            });
    });
}
