//! Assistant.swift: home, conversation, pending task and the large AskBox.
use super::{
    Ctx, DictationOwner, UiAction,
    components::{self as c, MarkdownStyle},
    icons::Icon,
    theme::{self as t, Weight},
};
use crate::model::{AssistantTurn, PanelMode, PendingTask};
use eframe::egui::{self, Align, Layout, Margin, Rect, Sense, Stroke, Ui};

pub fn render(ui: &mut Ui, cx: &mut Ctx, max_height: f32) {
    if cx.model.assistant.turns.is_empty() {
        home(ui, cx);
    } else {
        thread(ui, cx, max_height);
    }
    ask_box(ui, cx);
}

fn close(ui: &mut Ui, cx: &mut Ctx) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if c::icon_button(ui, "assistant-close", Icon::Close, 22.0, t::MUTED, false).on_hover_text("Close").clicked() {
            cx.emit(UiAction::SetPanel(None));
        }
    });
}

fn home(ui: &mut Ui, cx: &mut Ctx) {
    egui::Frame::new().inner_margin(Margin { left: 16, right: 16, top: 16, bottom: 8 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.horizontal(|ui| {
            c::label(ui, "Ask Lantern", 17.0, Weight::Bold, t::INK);
            close(ui, cx);
        });
        if cx.model.account.signed_in.is_some() {
            c::label(ui, "Ask anything, or tell Lantern what to do and an agent gets on it.", 13.0, Weight::Regular, t::MUTED);
            ui.add_space(2.0);
            for (icon, title) in [(Icon::Wifi, "Fix the Wi-Fi."), (Icon::Video, "Join the call."), (Icon::Photo, "Find that photo.")] {
                let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 37.0), Sense::click());
                ui.painter().rect_filled(rect, 11.0, t::FAINT);
                c::paint_icon(
                    ui.painter(),
                    Rect::from_center_size(egui::pos2(rect.left() + 23.0, rect.center().y), egui::Vec2::splat(14.0)),
                    icon,
                    t::AMBER,
                );
                let galley = ui.painter().layout_no_wrap(title.into(), t::font(14.0, Weight::Medium), t::white(0.9));
                ui.painter().galley(egui::pos2(rect.left() + 44.0, rect.center().y - galley.size().y / 2.0), galley, t::white(0.9));
                response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
                if response.clicked() {
                    cx.inputs.assistant = title.into();
                    cx.view.assistant_focus = None;
                }
            }
        }
    });
}

fn thread(ui: &mut Ui, cx: &mut Ctx, max_height: f32) {
    use std::hash::{Hash, Hasher};
    let anchor = cx.model.assistant.turns.last().map(|turn| {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        turn.answer.hash(&mut hash);
        (turn.id, hash.finish())
    });
    let scroll_to_last = anchor != cx.view.assistant_scroll_anchor;
    cx.view.assistant_scroll_anchor = anchor;
    egui::Frame::new().inner_margin(Margin { left: 16, right: 16, top: 14, bottom: 6 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.horizontal(|ui| {
            let galley = ui.painter().layout_no_wrap("New question".into(), t::font(12.0, Weight::Semibold), t::MUTED);
            let (rect, response) = ui.allocate_exact_size(egui::vec2(galley.size().x + 17.0, 22.0), Sense::click());
            c::paint_icon(
                ui.painter(),
                Rect::from_center_size(egui::pos2(rect.left() + 6.0, rect.center().y), egui::Vec2::splat(12.0)),
                Icon::Back,
                t::MUTED,
            );
            ui.painter().galley(egui::pos2(rect.left() + 17.0, rect.center().y - galley.size().y / 2.0), galley, t::MUTED);
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "New question"));
            if response.clicked() {
                cx.emit(UiAction::ClearAssistant);
            }
            close(ui, cx);
        });
        egui::ScrollArea::vertical()
            .id_salt("assistant-thread")
            .max_height((max_height - 220.0).max(200.0))
            .min_scrolled_height(0.0)
            .auto_shrink([false, true])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                egui::Frame::new().inner_margin(Margin::symmetric(0, 4)).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 14.0;
                    for turn in &cx.model.assistant.turns {
                        ui.push_id(turn.id, |ui| render_turn(ui, cx, turn));
                        if scroll_to_last && anchor.is_some_and(|(id, _)| id == turn.id) {
                            ui.scroll_to_cursor(Some(Align::BOTTOM));
                        }
                    }
                    if let Some(pending) = &cx.model.assistant.pending_task {
                        pending_card(ui, cx, pending);
                    }
                });
            });
    });
}

fn render_turn(ui: &mut Ui, cx: &mut Ctx, turn: &AssistantTurn) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        c::label(ui, &turn.question, 12.5, Weight::Semibold, t::INK);
        if turn.answer.is_empty() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                c::spinner(ui, 12.0, t::MUTED, cx.env);
                c::label(ui, "Thinking…", 12.0, Weight::Regular, t::MUTED);
            });
        } else {
            if let Some(link) = c::markdown_linked(
                ui,
                &turn.answer,
                MarkdownStyle { size: 12.5, color: if turn.failed { t::RED } else { t::white(0.92) }, ..Default::default() },
                2.0,
            )
            .1
            {
                cx.emit(UiAction::OpenUrl(link));
            }
        }
        if let Some(started) = &turn.started {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(12.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 6.0, t::GREEN);
                c::paint_icon(ui.painter(), rect.shrink(3.0), Icon::Play, t::black(0.85));
                c::label(ui, started, 11.5, Weight::Medium, t::GREEN);
            });
        }
        if let Some(note) = &turn.note {
            c::label(ui, note, 11.0, Weight::Regular, t::MUTED);
        }
    });
}

fn pending_card(ui: &mut Ui, cx: &mut Ctx, pending: &PendingTask) {
    egui::Frame::new()
        .fill(t::opacity(t::AMBER, 0.1))
        .stroke(Stroke::new(1.0, t::opacity(t::AMBER, 0.3)))
        .corner_radius(10)
        .inner_margin(Margin::same(9))
        .show(ui, |ui| {
            let controls = ["Cancel", "Start"]
                .iter()
                .map(|title| ui.painter().layout_no_wrap((*title).into(), t::font(11.5, Weight::Semibold), t::INK).size().x)
                .sum::<f32>();
            let text_width = (ui.available_width() - 12.0 - controls - 30.0).max(1.0);
            let folder = pending.folder.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("");
            let title = format!("Starting {} in {folder}… {}", pending.kind.display_name(), pending.seconds_left);
            let mut prompt = egui::text::LayoutJob::single_section(
                pending.prompt.clone(),
                egui::TextFormat { font_id: t::font(11.0, Weight::Regular), color: t::MUTED, ..Default::default() },
            );
            prompt.wrap.max_width = text_width;
            prompt.wrap.max_rows = 2;
            let prompt = ui.painter().layout_job(prompt);
            let height =
                ui.painter().layout(title.clone(), t::font(12.0, Weight::Semibold), t::INK, text_width).size().y + 2.0 + prompt.size().y;
            c::centered_row(ui, height, |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                c::spinner(ui, 12.0, t::MUTED, cx.env);
                ui.allocate_ui_with_layout(egui::vec2(text_width, height), Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    c::label(ui, title, 12.0, Weight::Semibold, t::INK);
                    ui.add(egui::Label::new(prompt));
                });
                if c::plain_button(ui, "Cancel", 11.5, Weight::Semibold, t::MUTED).clicked() {
                    cx.emit(UiAction::CancelAssistantTask);
                }
                if c::plain_button(ui, "Start", 11.5, Weight::Semibold, t::AMBER).clicked() {
                    cx.emit(UiAction::StartAssistantTaskNow);
                }
            });
        });
}

fn ask_box(ui: &mut Ui, cx: &mut Ctx) {
    egui::Frame::new().inner_margin(Margin { left: 12, right: 12, top: 4, bottom: 12 }).show(ui, |ui| {
        if cx.model.account.signed_in.is_none() {
            let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 33.0), Sense::click());
            ui.painter().rect_filled(rect, 12.0, t::FAINT);
            c::paint_icon(
                ui.painter(),
                Rect::from_center_size(egui::pos2(rect.left() + 17.0, rect.center().y), egui::Vec2::splat(12.0)),
                Icon::Sparkles,
                t::AMBER,
            );
            let galley =
                ui.painter().layout_no_wrap("Sign in to October to ask Lantern anything".into(), t::font(12.0, Weight::Medium), t::MUTED);
            ui.painter().galley(egui::pos2(rect.left() + 31.0, rect.center().y - galley.size().y / 2.0), galley, t::MUTED);
            c::paint_icon(
                ui.painter(),
                Rect::from_center_size(egui::pos2(rect.right() - 17.0, rect.center().y), egui::Vec2::splat(12.0)),
                Icon::ChevronRight,
                t::MUTED,
            );
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Sign in to October to ask Lantern anything"));
            if response.clicked() {
                cx.emit(UiAction::SetPanel(Some(PanelMode::October)));
            }
            return;
        }
        let id = ui.make_persistent_id("assistant-input");
        let focused = ui.memory(|mem| mem.has_focus(id));
        egui::Frame::new()
            .fill(t::black(0.3))
            .stroke(Stroke::new(1.0, if focused { t::opacity(t::AMBER, 0.5) } else { t::STROKE }))
            .corner_radius(12)
            .inner_margin(Margin { left: 13, right: 7, top: 10, bottom: 10 })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let width = ui.available_width() - 56.0;
                    let placeholder = if cx.model.dictating {
                        "Listening…"
                    } else if cx.model.assistant.turns.is_empty() {
                        "Ask Lantern, or tell it what to do"
                    } else {
                        "Ask a follow-up"
                    };
                    let edit = egui::ScrollArea::vertical()
                        .id_salt("assistant-input-scroll")
                        .max_height(6.0 * 18.0)
                        .min_scrolled_height(0.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut cx.inputs.assistant)
                                    .id(id)
                                    .font(t::font(15.0, Weight::Regular))
                                    .text_color(t::INK)
                                    .hint_text(c::text(placeholder, 15.0, Weight::Regular, t::MUTED))
                                    .frame(egui::Frame::NONE)
                                    .margin(0)
                                    .desired_width(width)
                                    .desired_rows(2)
                                    .return_key(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter)),
                            )
                        })
                        .inner;
                    if cx.view.assistant_focus != Some(cx.model.compose_focus) {
                        edit.request_focus();
                        cx.view.assistant_focus = Some(cx.model.compose_focus);
                    }
                    let submit = edit.has_focus() && c::consume_plain_key(ui, egui::Key::Enter);
                    ui.allocate_ui_with_layout(egui::vec2(50.0, edit.rect.height()), Layout::bottom_up(Align::Min), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if c::icon_button(
                                ui,
                                "assistant-mic",
                                if cx.model.dictating { Icon::MicFill } else { Icon::Mic },
                                22.0,
                                if cx.model.dictating { t::RED } else { t::MUTED },
                                false,
                            )
                            .on_hover_text(if cx.model.dictating { "Stop listening" } else { "Speak" })
                            .clicked()
                            {
                                cx.emit(UiAction::ToggleDictation { owner: DictationOwner::Assistant });
                            }
                            let enabled = !cx.inputs.assistant.trim().is_empty() && !cx.model.assistant.asking;
                            let response = ui
                                .add_enabled_ui(enabled, |ui| {
                                    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(22.0), Sense::click());
                                    ui.painter().circle_filled(rect.center(), 11.0, t::opacity(t::AMBER, if enabled { 1.0 } else { 0.35 }));
                                    c::paint_icon(ui.painter(), rect.shrink(5.5), Icon::ArrowUp, t::black(0.85));
                                    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, "Ask"));
                                    response
                                })
                                .inner
                                .on_hover_text("Ask (Return)");
                            if enabled && (submit || response.clicked()) {
                                cx.emit(UiAction::SubmitAssistant);
                            }
                        });
                    });
                });
            });
    });
}

#[cfg(test)]
mod tests {
    use super::super::{Assets, Env, UiState};
    use super::*;
    use crate::model::{Inputs, Model};

    fn frame(
        ctx: &egui::Context,
        assets: &Assets,
        model: &Model,
        inputs: &mut Inputs,
        view: &mut UiState,
        events: Vec<egui::Event>,
    ) -> (f32, Vec<UiAction>, Vec<(String, Rect)>) {
        let mut height = 0.0;
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(380.0, 900.0))),
                events,
                ..Default::default()
            },
            |ui| {
                height = super::super::render_panel(
                    ui,
                    &mut Ctx { model, inputs, view, assets, out: &mut actions, env: &Env::default() },
                    900.0,
                );
            },
        );
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((text.galley.job.text.clone(), Rect::from_min_size(text.pos, text.galley.size()))),
                _ => None,
            })
            .collect();
        output.textures_delta.clear();
        (height, actions, texts)
    }

    fn click(ctx: &egui::Context, assets: &Assets, model: &Model, inputs: &mut Inputs, view: &mut UiState, title: &str) -> Vec<UiAction> {
        let (_, _, texts) = frame(ctx, assets, model, inputs, view, Vec::new());
        let pos = texts.iter().find(|(text, _)| text == title).unwrap().1.center();
        let mut actions = Vec::new();
        for pressed in [true, false] {
            actions.extend(
                frame(
                    ctx,
                    assets,
                    model,
                    inputs,
                    view,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
                    ],
                )
                .1,
            );
        }
        actions
    }

    #[test]
    fn suggestions_prefill_without_submitting_and_return_keeps_input_until_ack() {
        let ctx = egui::Context::default();
        t::install(&ctx);
        let assets = Assets::load(&ctx);
        let mut fixture: crate::app::fixture::Fixture =
            serde_json::from_str(include_str!("../../fixtures/panel-assistant-home.json")).unwrap();
        let mut view = UiState::default();
        assert!(click(&ctx, &assets, &fixture.model, &mut fixture.inputs, &mut view, "Join the call.").is_empty());
        assert_eq!(fixture.inputs.assistant, "Join the call.");
        let (height, _, _) = frame(&ctx, &assets, &fixture.model, &mut fixture.inputs, &mut view, Vec::new());
        assert!(height > 250.0 && height < 450.0, "home height: {height}");
        let enter = || egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let (_, actions, _) = frame(&ctx, &assets, &fixture.model, &mut fixture.inputs, &mut view, vec![enter()]);
        assert_eq!(actions, vec![UiAction::SubmitAssistant]);
        assert_eq!(fixture.inputs.assistant, "Join the call.");
        fixture.model.assistant.asking = true;
        assert!(frame(&ctx, &assets, &fixture.model, &mut fixture.inputs, &mut view, vec![enter()]).1.is_empty());
    }

    #[test]
    fn signed_out_prompt_goes_to_october_and_never_submits() {
        let ctx = egui::Context::default();
        t::install(&ctx);
        let assets = Assets::load(&ctx);
        let mut fixture: crate::app::fixture::Fixture =
            serde_json::from_str(include_str!("../../fixtures/panel-assistant-signed-out.json")).unwrap();
        let actions = click(
            &ctx,
            &assets,
            &fixture.model,
            &mut fixture.inputs,
            &mut UiState::default(),
            "Sign in to October to ask Lantern anything",
        );
        assert_eq!(actions, vec![UiAction::SetPanel(Some(PanelMode::October))]);
    }

    #[test]
    fn pending_task_buttons_are_intents_not_countdowns_or_launches() {
        let ctx = egui::Context::default();
        t::install(&ctx);
        let assets = Assets::load(&ctx);
        let mut fixture: crate::app::fixture::Fixture =
            serde_json::from_str(include_str!("../../fixtures/panel-assistant-thread.json")).unwrap();
        let mut view = UiState::default();
        assert_eq!(click(&ctx, &assets, &fixture.model, &mut fixture.inputs, &mut view, "Cancel"), vec![UiAction::CancelAssistantTask]);
        assert_eq!(click(&ctx, &assets, &fixture.model, &mut fixture.inputs, &mut view, "Start"), vec![UiAction::StartAssistantTaskNow]);
        assert_eq!(fixture.model.assistant.pending_task.as_ref().unwrap().seconds_left, 3);
        assert_eq!(fixture.model.assistant.turns[1].started, None);
        assert_eq!(click(&ctx, &assets, &fixture.model, &mut fixture.inputs, &mut view, "New question"), vec![UiAction::ClearAssistant]);
    }
}
