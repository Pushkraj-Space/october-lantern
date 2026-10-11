//! Shared primitives with the Swift geometry, colors and interaction states.
use super::{
    assets::Assets,
    icons::Icon,
    theme::{self, Weight},
};
use crate::model::{Agent, AgentKind};
use eframe::egui::{self, Color32, Id, Label, Painter, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, Ui, Vec2};

pub use super::icons::paint_icon;

pub fn text(text: impl Into<String>, size: f32, weight: Weight, color: Color32) -> RichText {
    RichText::new(text).font(theme::font(size, weight)).color(color)
}

pub fn label(ui: &mut Ui, value: impl Into<String>, size: f32, weight: Weight, color: Color32) -> Response {
    ui.add(Label::new(text(value, size, weight, color)).wrap())
}

/// Plain button with no egui chrome. `size` is the hit box, the glyph sits inside it.
pub fn icon_button(ui: &mut Ui, id: impl egui::AsIdSalt, icon: Icon, size: f32, tint: Color32, active: bool) -> Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let response = ui.interact(rect, ui.make_persistent_id(id), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), format!("{icon:?}")));
    if active || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, if size >= 32.0 { size / 2.0 } else { 9.0 }, theme::white(0.12));
    }
    let glyph = Rect::from_center_size(rect.center(), Vec2::splat(if size <= 22.0 { size * 0.5 } else { 14.0 }));
    paint_icon(ui.painter(), glyph, icon, tint);
    response
}

/// Swift `SmallButton`: 11 medium, icon/text gap, horizontal 8 / vertical 4, hover capsule.
pub fn small_button(ui: &mut Ui, title: &str, icon: Icon) -> Response {
    let galley = ui.painter().layout_no_wrap(title.into(), theme::font(11.0, Weight::Medium), theme::MUTED);
    let size = egui::vec2(16.0 + 11.0 + 4.0 + galley.size().x, galley.size().y + 8.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), title));
    let color = if response.hovered() || response.has_focus() { theme::INK } else { theme::MUTED };
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, rect.height() / 2.0, theme::FAINT);
    }
    paint_icon(ui.painter(), Rect::from_center_size(egui::pos2(rect.left() + 13.5, rect.center().y), Vec2::splat(11.0)), icon, color);
    ui.painter().galley_with_override_text_color(egui::pos2(rect.left() + 23.0, rect.center().y - galley.size().y / 2.0), galley, color);
    response
}

/// A text-only plain button, retaining native keyboard focus and accessible button semantics.
pub fn text_button(ui: &mut Ui, title: &str, size: f32, weight: Weight, color: Color32) -> Response {
    ui.add(egui::Button::new(text(title, size, weight, color)).frame(false).min_size(Vec2::ZERO))
}

/// Glass paint only: a dark tint plus source rim/sheen. Native live backdrop material is the
/// shell's responsibility. Insert at a reserved paint slot behind content once its height is known.
pub fn glass_surface(ui: &mut Ui, rect: Rect, radius: f32, tint: f32) {
    paint_glass(ui.painter(), rect, radius, tint);
}

pub fn paint_glass(painter: &Painter, rect: Rect, radius: f32, tint: f32) {
    // Swift's glassSurface adds .35 black over GlassContainer's native tint.
    let alpha = 1.0 - (1.0 - tint.clamp(0.0, 1.0)) * 0.65;
    painter.rect_filled(rect, radius, theme::black(alpha));
    let perimeter = rounded_points(rect.shrink(0.5), radius.min(rect.width() / 2.0).min(rect.height() / 2.0));
    let color = |y: f32, rim: bool| {
        let t = ((y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
        let a = if rim {
            if t <= 0.5 { 0.42 + (0.10 - 0.42) * t * 2.0 } else { 0.10 + (0.18 - 0.10) * (t - 0.5) * 2.0 }
        } else {
            0.10 * (1.0 - t * 2.0).max(0.0)
        };
        theme::white(a)
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.center(), Color32::TRANSPARENT);
    for point in &perimeter {
        mesh.colored_vertex(*point, color(point.y, false));
    }
    for i in 0..perimeter.len() {
        mesh.add_triangle(0, i as u32 + 1, ((i + 1) % perimeter.len()) as u32 + 1);
    }
    painter.add(Shape::mesh(mesh));
    for i in 0..perimeter.len() {
        let a = perimeter[i];
        let b = perimeter[(i + 1) % perimeter.len()];
        painter.line_segment([a, b], Stroke::new(1.0, color((a.y + b.y) * 0.5, true)));
    }
}

fn rounded_points(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius.max(0.0);
    let centers = [
        (rect.right() - r, rect.top() + r, -std::f32::consts::FRAC_PI_2),
        (rect.right() - r, rect.bottom() - r, 0.0),
        (rect.left() + r, rect.bottom() - r, std::f32::consts::FRAC_PI_2),
        (rect.left() + r, rect.top() + r, std::f32::consts::PI),
    ];
    centers
        .into_iter()
        .flat_map(|(x, y, start)| {
            (0..=12).map(move |i| {
                let a = start + i as f32 * std::f32::consts::FRAC_PI_2 / 12.0;
                egui::pos2(x + r * a.cos(), y + r * a.sin())
            })
        })
        .collect()
}

pub fn harness_logo(ui: &mut Ui, assets: &Assets, kind: &AgentKind, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if let Some(texture) = assets.harness(kind) {
        ui.put(rect, egui::Image::new(texture).fit_to_exact_size(rect.size()).corner_radius(size * 0.22));
    } else {
        ui.painter().rect_filled(rect, size * 0.22, theme::FAINT);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            kind.display_name().chars().next().unwrap_or('?'),
            theme::font(size * 0.46, Weight::Rounded),
            theme::INK,
        );
    }
    response.on_hover_text(kind.display_name())
}

pub fn agent_badge(ui: &mut Ui, assets: &Assets, agent: &Agent, size: f32) -> Response {
    let response = harness_logo(ui, assets, &agent.kind, size);
    let center = response.rect.right_bottom() + egui::vec2(2.0 - size * 0.17, 2.0 - size * 0.17);
    ui.painter().circle(center, size * 0.17, theme::state_color(agent.state), Stroke::new(1.5, theme::black(0.7)));
    response
}

pub fn state_chip(ui: &mut Ui, agent: &Agent) -> Response {
    if !agent.is_live() {
        return egui::Frame::new()
            .fill(theme::FAINT)
            .corner_radius(20)
            .inner_margin(egui::Margin::symmetric(7, 2))
            .show(ui, |ui| label(ui, "Recent", 10.5, Weight::Medium, theme::MUTED))
            .response;
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(6.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 3.0, theme::state_color(agent.state));
        label(ui, theme::state_label(agent.state), 11.0, Weight::Medium, if agent.state.wants_you() { theme::AMBER } else { theme::MUTED });
    })
    .response
}

#[derive(Clone, Copy, Debug)]
pub struct MarkdownStyle {
    pub size: f32,
    pub color: Color32,
    pub max_rows: usize,
}
impl Default for MarkdownStyle {
    fn default() -> Self {
        Self { size: 12.5, color: theme::white(0.9), max_rows: usize::MAX }
    }
}

/// Inline-only, preserving whitespace like Swift's AttributedString. Full chat block rendering
/// uses `blocks` below instead of feeding arbitrary agent HTML to a renderer.
pub fn markdown(ui: &mut Ui, source: &str, style: MarkdownStyle) -> Response {
    let mut job = inline_job(source, style);
    job.wrap.max_width = ui.available_width();
    job.wrap.max_rows = style.max_rows;
    ui.add(Label::new(job).selectable(true))
}

pub fn inline_job(source: &str, style: MarkdownStyle) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let mut rest = source;
    while !rest.is_empty() {
        let delimiter = if rest.starts_with('`') {
            Some(("`", Weight::Mono, false))
        } else if rest.starts_with("**") {
            Some(("**", Weight::Semibold, false))
        } else if rest.starts_with("__") {
            Some(("__", Weight::Semibold, false))
        } else if rest.starts_with('*') {
            Some(("*", Weight::Regular, true))
        } else if rest.starts_with('_') {
            Some(("_", Weight::Regular, true))
        } else {
            None
        };
        if let Some((marker, weight, italics)) = delimiter
            && let Some(end) = rest[marker.len()..].find(marker)
            && end > 0
        {
            let body = &rest[marker.len()..marker.len() + end];
            job.append(
                body,
                0.0,
                egui::TextFormat {
                    font_id: theme::font(if weight == Weight::Mono { style.size - 1.0 } else { style.size }, weight),
                    color: style.color,
                    italics,
                    background: if weight == Weight::Mono { theme::black(0.2) } else { Color32::TRANSPARENT },
                    ..Default::default()
                },
            );
            rest = &rest[marker.len() * 2 + end..];
        } else if let Some(stripped) = rest.strip_prefix('\\') {
            if let Some(ch) = stripped.chars().next() {
                job.append(&ch.to_string(), 0.0, format(style));
                rest = &stripped[ch.len_utf8()..];
            } else {
                job.append(rest, 0.0, format(style));
                break;
            }
        } else {
            let len = rest.chars().next().unwrap().len_utf8();
            job.append(&rest[..len], 0.0, format(style));
            rest = &rest[len..];
        }
    }
    job
}

fn format(style: MarkdownStyle) -> egui::TextFormat {
    egui::TextFormat { font_id: theme::font(style.size, Weight::Regular), color: style.color, ..Default::default() }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Paragraph(String),
    Heading(String),
    Item { marker: String, text: String },
    Code(String),
}

/// Port of `MarkdownBlocks.parse`, including incomplete streamed code fences.
pub fn parse_blocks(source: &str) -> Vec<Block> {
    fn flush(paragraph: &mut Vec<&str>, out: &mut Vec<Block>) {
        if !paragraph.is_empty() {
            out.push(Block::Paragraph(paragraph.join("\n")));
            paragraph.clear();
        }
    }
    let mut out = Vec::new();
    let mut paragraph = Vec::new();
    let mut code: Option<Vec<&str>> = None;
    for line in source.split('\n') {
        let trimmed = line.trim();
        if let Some(lines) = &mut code {
            if trimmed.starts_with("```") {
                out.push(Block::Code(lines.join("\n")));
                code = None;
            } else {
                lines.push(line);
            }
        } else if trimmed.starts_with("```") {
            flush(&mut paragraph, &mut out);
            code = Some(Vec::new());
        } else if trimmed.is_empty() {
            flush(&mut paragraph, &mut out);
        } else if trimmed.starts_with('#') {
            flush(&mut paragraph, &mut out);
            out.push(Block::Heading(trimmed.trim_start_matches('#').trim().into()));
        } else if let Some(body) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.strip_prefix("• "))
        {
            flush(&mut paragraph, &mut out);
            out.push(Block::Item { marker: "•".into(), text: body.into() });
        } else if let Some((number, body)) = trimmed.split_once(". ")
            && !number.is_empty()
            && number.chars().all(char::is_numeric)
        {
            flush(&mut paragraph, &mut out);
            out.push(Block::Item { marker: format!("{number}."), text: body.into() });
        } else {
            paragraph.push(line);
        }
    }
    if let Some(code) = code {
        out.push(Block::Code(code.join("\n")));
    }
    flush(&mut paragraph, &mut out);
    out
}

/// Full conversation Markdown, returning the code to copy (the caller emits UiAction::Copy).
pub fn blocks(ui: &mut Ui, source: &str, id: Id) -> Option<String> {
    let mut copy = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        for (i, block) in parse_blocks(source).into_iter().enumerate() {
            match block {
                Block::Paragraph(body) => {
                    markdown(ui, &body, MarkdownStyle::default());
                }
                Block::Heading(body) => {
                    label(ui, body, 13.0, Weight::Semibold, theme::INK);
                }
                Block::Item { marker, text: body } => {
                    ui.horizontal_top(|ui| {
                        label(ui, marker, 12.5, Weight::Regular, theme::MUTED);
                        markdown(ui, &body, MarkdownStyle::default());
                    });
                }
                Block::Code(body) => {
                    egui::Frame::new()
                        .fill(theme::black(0.3))
                        .stroke(Stroke::new(1.0, theme::HAIRLINE))
                        .corner_radius(8)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                let width = (ui.available_width() - 24.0).max(0.0);
                                egui::ScrollArea::horizontal().id_salt((id, i)).max_width(width).show(ui, |ui| {
                                    ui.add(Label::new(text(&body, 11.5, Weight::Mono, theme::white(0.9))).extend().selectable(true));
                                });
                                if icon_button(ui, (id, i, "copy"), Icon::Copy, 22.0, theme::MUTED, false)
                                    .on_hover_text("Copy code")
                                    .clicked()
                                {
                                    copy = Some(body.clone());
                                }
                            });
                        });
                }
            }
        }
    });
    copy
}

pub fn time_ago(since_ms: Option<f64>, now_ms: f64) -> String {
    let Some(since) = since_ms else {
        return String::new();
    };
    let seconds = ((now_ms - since) / 1000.0).max(0.0) as u64;
    match seconds {
        0..60 => "now".into(),
        60..3600 => format!("{}m", seconds / 60),
        3600..86400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_fonts_art_and_vector_controls_render_without_host_resources() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let assets = Assets::load(&ctx);
        let mut output = ctx.run_ui(
            egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(700.0, 900.0))), ..Default::default() },
            |ui| {
                paint_glass(ui.painter(), Rect::from_min_size(Pos2::ZERO, egui::vec2(380.0, 600.0)), 22.0, 0.5);
                for weight in [Weight::Regular, Weight::Medium, Weight::Semibold, Weight::Bold, Weight::Rounded, Weight::Mono] {
                    label(ui, "Lantern · café Ελληνικά", 12.5, weight, theme::INK);
                }
                for kind in AgentKind::SUPPORTED {
                    let agent = Agent { kind: AgentKind::new(kind), ..Default::default() };
                    assert!(assets.harness(&agent.kind).is_some());
                    agent_badge(ui, &assets, &agent, 26.0);
                }
                use Icon::*;
                ui.horizontal_wrapped(|ui| {
                    for icon in [
                        Sparkles,
                        Layers,
                        Plus,
                        PointAsk,
                        Close,
                        Back,
                        ChevronDown,
                        ChevronUpDown,
                        Reply,
                        Open,
                        Dismiss,
                        Mic,
                        MicFill,
                        Waveform,
                        Send,
                        ArrowUp,
                        Return,
                        Cpu,
                        Folder,
                        Terminal,
                        Chat,
                        Copy,
                        Check,
                        Hourglass,
                        Warning,
                        Reselect,
                        Link,
                        TextCursor,
                        Wifi,
                        Video,
                        Photo,
                        Settings,
                        Leaf,
                        Info,
                        Account,
                        Mail,
                        Play,
                        Exclamation,
                    ] {
                        icon_button(ui, icon, icon, 32.0, theme::MUTED, false);
                    }
                });
                markdown(ui, "**Hello**\n`let x = 1;`", MarkdownStyle::default());
            },
        );
        assert!(!output.textures_delta.set.is_empty());
        assert!(!ctx.tessellate(output.shapes, output.pixels_per_point).is_empty());
        // This headless test intentionally has no GPU backend to apply uploads to.
        output.textures_delta.clear();
    }
    #[test]
    fn blocks_match_swift_and_keep_streamed_code() {
        assert_eq!(
            parse_blocks("# Title\n\na\nb\n- x\n2. y\n```rust\nlet x = 1;"),
            vec![
                Block::Heading("Title".into()),
                Block::Paragraph("a\nb".into()),
                Block::Item { marker: "•".into(), text: "x".into() },
                Block::Item { marker: "2.".into(), text: "y".into() },
                Block::Code("let x = 1;".into())
            ]
        );
    }
    #[test]
    fn inline_keeps_whitespace_unicode_and_unclosed_delimiters() {
        let j = inline_job("hi **bold**\n`code` café *cut", MarkdownStyle::default());
        assert_eq!(j.text, "hi bold\ncode café *cut");
        assert!(j.sections.iter().any(|s| s.format.font_id == theme::font(12.5, Weight::Semibold)));
    }
    #[test]
    fn relative_time_is_fixed_clock_and_clamped() {
        assert_eq!(time_ago(None, 1000.0), "");
        assert_eq!(time_ago(Some(2000.0), 1000.0), "now");
        assert_eq!(time_ago(Some(0.0), 3600000.0), "1h");
    }
}
