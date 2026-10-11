//! The pill (root viewport) and the panel beside it (a second native window), positioned and
//! behaving like the Mac's (CONTRACT §6). Geometry is `shell`'s; platform roles are `os::shell`'s.
use super::Lantern;
use crate::os::shell::{self as platform, PANEL_TITLE, PILL_TITLE, Role};
use crate::shell::{self, Hover, PILL_COLLAPSED, PILL_EXPANDED};
use crate::ui::{self, Ctx, Env};
use eframe::egui::{self, Pos2, Rect, Vec2, ViewportBuilder, ViewportCommand, ViewportId, vec2};
use std::time::{Duration, Instant};

/// Swift: the panel fades in over .12 s; a snap animates over .18 s.
const FADE: f32 = 0.12;
const SNAP: f32 = 0.18;
/// How often the work areas are re-read (monitors and panels change rarely).
const AREAS_EVERY: Duration = Duration::from_secs(2);

struct Drag {
    window_start: Pos2,
    pointer_start: Pos2,
}

struct Snap {
    from: Pos2,
    started: Instant,
}

#[derive(Default)]
pub struct Windows {
    hover: Hover,
    /// Work areas in points, and when they were read.
    areas: Vec<Rect>,
    areas_at: Option<Instant>,
    pill_adopted: bool,
    panel_adopted: bool,
    frames: u64,
    /// The pill's position and size as last sent to the window.
    pill_at: Option<Pos2>,
    pill_size: Option<Vec2>,
    drag: Option<Drag>,
    snap: Option<Snap>,
    panel_height: f32,
    panel_opened: Option<Instant>,
    panel_focused: bool,
    /// A dialog Lantern opened (folder picker, capture permission) is up: losing focus to it
    /// doesn't close the panel.
    pub native_dialog: bool,
    clipboard: Option<String>,
    quitting: bool,
}

impl Windows {
    pub fn animating(&self) -> bool {
        self.drag.is_some()
            || self.snap.is_some()
            || self.panel_opened.is_some_and(|at| at.elapsed().as_secs_f32() < FADE)
            || self.hover.expanded
    }
    pub fn copy(&mut self, text: String) {
        self.clipboard = Some(text);
    }
    pub fn quit(&mut self) {
        self.quitting = true;
    }
}

/// Work areas in points (the platform reports physical pixels).
fn areas_in_points(scale: f32) -> Vec<Rect> {
    platform::work_areas()
        .into_iter()
        .map(|area| Rect::from_min_max((area.min.to_vec2() / scale).to_pos2(), (area.max.to_vec2() / scale).to_pos2()))
        .collect()
}

/// The root viewport: the pill, collapsed, where the Mac would put it.
pub fn options() -> eframe::NativeOptions {
    let prefs = crate::model::Prefs::default(); // TODO(apollo): persisted edge/top
    let start = areas_in_points(1.0)
        .first()
        .map(|work| shell::pill_frame(*work, PILL_COLLAPSED, prefs.edge, prefs.pill_top).min)
        .unwrap_or(Pos2::new(100.0, 100.0));
    #[allow(unused_mut)]
    let mut viewport = ViewportBuilder::default()
        .with_title(PILL_TITLE)
        .with_app_id("dev.october.lantern")
        .with_inner_size(PILL_COLLAPSED)
        .with_position(start)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false);
    #[cfg(target_os = "linux")]
    {
        // Proven in Phase 0: a dock window never takes focus; clicks still arrive.
        viewport = viewport.with_window_type(egui::X11WindowType::Dock);
    }
    eframe::NativeOptions {
        viewport,
        #[cfg(target_os = "linux")]
        event_loop_builder: platform::x11_available().then(|| -> eframe::EventLoopBuilderHook {
            Box::new(|builder| {
                use winit::platform::x11::EventLoopBuilderExtX11;
                builder.with_x11();
            })
        }),
        ..Default::default()
    }
}

impl Lantern {
    /// One frame of the pill and, when open, the panel.
    pub(super) fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let w = &mut self.windows;
        w.frames += 1;
        let scale = ctx.native_pixels_per_point().unwrap_or(1.0);
        if w.areas_at.is_none_or(|at| at.elapsed() >= AREAS_EVERY) {
            w.areas = areas_in_points(scale);
            w.areas_at = Some(Instant::now());
        }
        if let Some(text) = w.clipboard.take() {
            ctx.copy_text(text);
        }
        if w.quitting {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        // Give the native windows their roles once they exist (on top, every workspace, no
        // taskbar; the pill never takes focus).
        if !w.pill_adopted && w.frames.is_multiple_of(5) {
            w.pill_adopted = platform::adopt(PILL_TITLE, Role::Pill);
        }

        // Hover: our own window's pointer events; on Windows also the polled 6pt halo.
        let now = ctx.input(|input| input.time);
        let outer = ctx.input(|input| input.viewport().outer_rect);
        let inside_window = ctx.input(|input| input.pointer.hover_pos().is_some());
        let in_halo = self.model.caps.hover_halo
            && platform::cursor_position()
                .zip(outer)
                .is_some_and(|(cursor, rect)| rect.expand(6.0).contains((cursor.to_vec2() / scale).to_pos2()));
        if w.drag.is_none() {
            let was = w.hover.expanded;
            w.hover.update(inside_window || in_halo, now);
            if was != w.hover.expanded {
                super::trace!("hover expanded={}", w.hover.expanded);
            }
        }
        let expanded = shell::pill_expanded(&w.hover, self.model.panel.is_some(), self.model.dictating);
        self.model.pill_expanded = expanded;

        // Where the pill goes.
        let size = if expanded { PILL_EXPANDED } else { PILL_COLLAPSED };
        let current = outer.map_or(Pos2::ZERO, |rect| rect.center());
        let work = shell::work_area_at(&w.areas, current).unwrap_or(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0)));
        let home = shell::pill_frame(work, size, self.model.prefs.edge, self.model.prefs.pill_top).min;
        let at = match (&w.drag, &w.snap) {
            (Some(_), _) => w.pill_at.unwrap_or(home),
            (None, Some(snap)) => {
                let t = (snap.started.elapsed().as_secs_f32() / SNAP).min(1.0);
                let eased = 1.0 - (1.0 - t).powi(3);
                snap.from + (home - snap.from) * eased
            }
            (None, None) => home,
        };
        if w.snap.as_ref().is_some_and(|snap| snap.started.elapsed().as_secs_f32() >= SNAP) {
            w.snap = None;
        }
        if w.pill_size != Some(size) {
            // A fixed-size window: min and max follow the size, or the window manager may keep
            // enforcing the expanded size after the pill collapses.
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(size));
            ctx.send_viewport_cmd(ViewportCommand::MaxInnerSize(size));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
            w.pill_size = Some(size);
        }
        if w.pill_at.is_none_or(|old| (old - at).length() > 0.5) {
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(at));
            w.pill_at = Some(at);
        }

        // The pill.
        let assets = self.assets.as_ref().expect("assets load before the first frame");
        let input = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| shell::pill::show(ui, &self.model, assets, expanded, &mut self.actions))
            .inner;
        let w = &mut self.windows;
        // Dragging by the lantern moves the pill (and the panel with it); letting go snaps it to
        // the nearer edge. Positions come from the window, never the global pointer (CONTRACT §8.2).
        let pointer = ctx.input(|input| input.pointer.interact_pos()).zip(outer).map(|(local, rect)| rect.min + local.to_vec2());
        if input.drag_started
            && let (Some(pointer), Some(window)) = (pointer, w.pill_at)
        {
            super::trace!("drag start at {window:?}");
            w.drag = Some(Drag { window_start: window, pointer_start: pointer });
            w.snap = None;
        }
        if input.dragging
            && let (Some(drag), Some(pointer)) = (&w.drag, pointer)
        {
            let to = drag.window_start + (pointer - drag.pointer_start);
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(to));
            w.pill_at = Some(to);
        }
        if input.drag_stopped && w.drag.take().is_some() {
            let dropped = Rect::from_min_size(w.pill_at.unwrap_or(home), size);
            let (edge, top) = shell::snap(dropped, work);
            super::trace!("drag stop at {:?} -> snap {edge:?} top {top:.3}", dropped.min);
            self.model.prefs.edge = edge;
            self.model.prefs.pill_top = top;
            w.snap = Some(Snap { from: dropped.min, started: Instant::now() });
            // TODO(apollo): persist prefs.
        }

        self.panel(&ctx, work, scale);
    }

    /// The panel window beside the pill, while a panel is open.
    fn panel(&mut self, ctx: &egui::Context, work: Rect, _scale: f32) {
        let w = &mut self.windows;
        if self.model.panel.is_none() {
            w.panel_opened = None;
            w.panel_adopted = false;
            w.panel_focused = false;
            return;
        }
        let opened = *w.panel_opened.get_or_insert_with(Instant::now);
        let pill = Rect::from_min_size(w.pill_at.unwrap_or_default(), w.pill_size.unwrap_or(PILL_COLLAPSED));
        let edge = self.model.prefs.edge;
        let wanted = vec2(ui::theme::PANEL_WIDTH, if w.panel_height > 0.0 { w.panel_height } else { 200.0 });
        let frame = shell::panel_frame(pill, wanted, edge, work);
        if !w.panel_adopted && w.frames.is_multiple_of(5) {
            w.panel_adopted = platform::adopt(PANEL_TITLE, Role::Panel);
        }
        let builder = ViewportBuilder::default()
            .with_title(PANEL_TITLE)
            .with_inner_size(frame.size())
            .with_position(frame.min)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false);
        let max_height = shell::panel_limit(work);
        let env = Env {
            now_ms: Self::now_ms(),
            pixels_per_point: ctx.pixels_per_point(),
            reduce_motion: false,
            home: crate::settings::home().to_string_lossy().into_owned(),
        };
        let (model, inputs, view, out) = (&self.model, &mut self.inputs, &mut self.view, &mut self.actions);
        let assets = self.assets.as_ref().expect("assets load before the first frame");
        let mut measured = None;
        let mut focused = None;
        ctx.show_viewport_immediate(ViewportId::from_hash_of("lantern-panel"), builder, |ui, _class| {
            ctx_commands(ui.ctx(), frame);
            let fade = (opened.elapsed().as_secs_f32() / FADE).min(1.0);
            ui.set_opacity(fade);
            // The panel surface draws its own glass and handles Esc (ui::render_panel).
            let mut cx = Ctx { model, inputs, view, out, assets, env: &env };
            let height =
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| ui::render_panel(ui, &mut cx, max_height)).inner;
            measured = Some(height);
            focused = ui.input(|input| input.viewport().focused);
        });
        let w = &mut self.windows;
        if let Some(height) = measured
            && (height - w.panel_height).abs() > 0.5
        {
            w.panel_height = height.min(max_height);
        }
        // Losing focus closes an idle panel (a proxy for the Mac's outside click, CONTRACT §6).
        if let Some(now_focused) = focused {
            if w.panel_focused != now_focused {
                super::trace!("panel focused={now_focused}");
            }
            if w.panel_focused && !now_focused {
                let draft_empty = self.inputs.drafts.is_empty();
                if shell::closes_on_focus_loss(draft_empty, self.model.dictating, w.native_dialog, false) {
                    self.actions.push(crate::ui::UiAction::SetPanel(None));
                }
            }
            w.panel_focused = now_focused;
        }
    }
}

/// Keeps the panel window where and as big as `frame` says.
fn ctx_commands(ctx: &egui::Context, frame: Rect) {
    let (outer, inner) = ctx.input(|input| (input.viewport().outer_rect, input.viewport().inner_rect));
    if outer.is_none_or(|rect| (rect.min - frame.min).length() > 0.5) {
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(frame.min));
    }
    if inner.is_none_or(|rect| (rect.size() - frame.size()).length() > 0.5) {
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(frame.size()));
    }
}
