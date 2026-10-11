//! CommonMark inline interpretation with Swift's whitespace-preserving presentation.
//! HTML is text, never executable; link clicks return intents, never platform output commands.
use super::{
    components::MarkdownStyle,
    theme::{self as t, Weight},
};
use eframe::egui::{self, Color32, Label, Rect, Response, Sense, Stroke, Ui};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::{
    hash::{Hash, Hasher},
    ops::Range,
};

#[derive(Debug)]
struct Link {
    bytes: Range<usize>,
    url: String,
}

struct Inline {
    job: egui::text::LayoutJob,
    links: Vec<Link>,
}

fn safe_url(value: &str) -> Option<String> {
    if value.chars().any(char::is_control) {
        return None;
    }
    let url = url::Url::parse(value).ok()?;
    match url.scheme() {
        "http" | "https" if url.host_str().is_some() => Some(url.into()),
        "mailto" if !url.path().is_empty() => Some(url.into()),
        _ => None,
    }
}

fn parse(source: &str, style: MarkdownStyle) -> Inline {
    // A non-Markdown punctuation prefix makes every line part of one paragraph: headings,
    // lists, indents and fences stay literal in inline-only mode. Blank/trailing lines survive.
    // Choose a deterministic marker absent from the input; no genuine user character is removed.
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hash);
    let mut marker = format!("§{:016x}§", hash.finish());
    while source.contains(&marker) {
        marker.push('§');
    }
    let neutral = source.split('\n').map(|line| format!("{marker}{line}")).collect::<Vec<_>>().join("\n");
    let mut result = Inline { job: egui::text::LayoutJob::default(), links: Vec::new() };
    let mut strong: usize = 0;
    let mut emphasis: usize = 0;
    let mut link: Option<String> = None;
    for (event, span) in Parser::new(&neutral).into_offset_iter() {
        let (value, code) = match event {
            Event::Start(Tag::Strong) => {
                strong += 1;
                continue;
            }
            Event::End(TagEnd::Strong) => {
                strong = strong.saturating_sub(1);
                continue;
            }
            Event::Start(Tag::Emphasis) => {
                emphasis += 1;
                continue;
            }
            Event::End(TagEnd::Emphasis) => {
                emphasis = emphasis.saturating_sub(1);
                continue;
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                link = safe_url(&dest_url);
                continue;
            }
            Event::End(TagEnd::Link) => {
                link = None;
                continue;
            }
            Event::Text(value) | Event::InlineHtml(value) | Event::Html(value) => (value.replace(&marker, ""), false),
            Event::Code(value) => (value.replace(&marker, ""), true),
            Event::SoftBreak => ("\n".into(), false),
            Event::HardBreak => {
                // Preserve the spaces of a whitespace hard break, but not an escaped newline's
                // syntax backslash. CommonMark otherwise discards both before emitting this event.
                let raw = neutral[span].trim_end_matches(['\n', '\r']);
                (if raw.ends_with('\\') { "\n".into() } else { format!("{raw}\n") }, false)
            }
            _ => continue,
        };
        if value.is_empty() {
            continue;
        }
        let start = result.job.text.len();
        let format = egui::TextFormat {
            font_id: t::font(
                if code { style.size - 1.0 } else { style.size },
                if code {
                    Weight::Mono
                } else if strong > 0 {
                    Weight::Semibold
                } else {
                    Weight::Regular
                },
            ),
            color: if link.is_some() { t::LINK } else { style.color },
            italics: emphasis > 0 && !code,
            background: if code { t::black(0.2) } else { Color32::TRANSPARENT },
            ..Default::default()
        };
        result.job.append(&value, 0.0, format);
        if let Some(url) = &link {
            let end = result.job.text.len();
            if let Some(last) = result.links.last_mut().filter(|last| last.url == *url && last.bytes.end == start) {
                last.bytes.end = end;
            } else {
                result.links.push(Link { bytes: start..end, url: url.clone() });
            }
        }
    }
    result
}

#[cfg(test)]
pub(super) fn inline_job(source: &str, style: MarkdownStyle) -> egui::text::LayoutJob {
    parse(source, style).job
}

fn link_rects(galley: &egui::Galley, origin: egui::Pos2, chars: Range<usize>) -> Vec<Rect> {
    let mut offset = 0;
    let mut rects = Vec::new();
    for row in &galley.rows {
        let end = offset + row.glyphs.len();
        let start = chars.start.max(offset);
        let stop = chars.end.min(end);
        if start < stop {
            let left = row.glyphs[start - offset].pos.x;
            let right = row.glyphs.get(stop - offset).map_or(row.size.x, |glyph| glyph.pos.x);
            rects.push(Rect::from_min_size(origin + row.pos.to_vec2() + egui::vec2(left, 0.0), egui::vec2(right - left, row.size.y)));
        }
        offset = end + usize::from(row.ends_with_newline);
    }
    rects
}

pub fn render(ui: &mut Ui, source: &str, style: MarkdownStyle, line_spacing: f32) -> (Response, Option<String>) {
    let mut parsed = parse(source, style);
    if line_spacing > 0.0 {
        let height = ui.fonts_mut(|fonts| fonts.row_height(&t::font(style.size, Weight::Regular))) + line_spacing;
        for section in &mut parsed.job.sections {
            section.format.line_height = Some(height);
        }
    }
    parsed.job.wrap.max_width = ui.available_width();
    parsed.job.wrap.max_rows = style.max_rows;
    let galley = ui.painter().layout_job(parsed.job);
    let response = ui.add(Label::new(galley.clone()).selectable(true));
    let mut clicked = None;
    for (index, link) in parsed.links.iter().enumerate() {
        let chars = galley.job.text[..link.bytes.start].chars().count()..galley.job.text[..link.bytes.end].chars().count();
        for (row, rect) in link_rects(&galley, response.rect.min, chars).into_iter().enumerate() {
            let rect = rect.intersect(ui.clip_rect());
            if !rect.is_positive() {
                continue;
            }
            let hit =
                ui.interact(rect, response.id.with(("link", &link.url, index, row)), if row == 0 { Sense::click() } else { Sense::CLICK });
            if row == 0 {
                hit.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Link, ui.is_enabled(), &galley.job.text[link.bytes.clone()])
                });
            }
            if hit.clicked() {
                clicked = Some(link.url.clone());
            }
            if hit.has_focus() {
                ui.painter().rect_stroke(rect.expand(1.0), 2.0, Stroke::new(1.0, t::LINK), egui::StrokeKind::Outside);
            }
            hit.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(&link.url);
        }
    }
    (response, clicked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commonmark_nests_styles_and_preserves_literal_block_syntax_and_whitespace() {
        let style = MarkdownStyle::default();
        let parsed = parse("  **bold *nested*** and _italic_ `a * b`\\*\n\n# literal\n- item\n```rust\n끝\n", style);
        assert_eq!(parsed.job.text, "  bold nested and italic a * b*\n\n# literal\n- item\n```rust\n끝\n");
        assert!(
            parsed
                .job
                .sections
                .iter()
                .any(|section| section.format.italics && section.format.font_id == t::font(style.size, Weight::Semibold))
        );
        assert!(parsed.job.sections.iter().any(|section| section.format.font_id == t::font(style.size - 1.0, Weight::Mono)));
        assert_eq!(inline_job("x  \ny\\\nz", style).text, "x  \ny\nz");
        assert_eq!(inline_job("café snake_case **unfinished", style).text, "café snake_case **unfinished");
        assert_eq!(inline_job("§ _one_\n**two\nthree**", style).text, "§ one\ntwo\nthree");
    }

    #[test]
    fn links_decode_targets_but_html_images_and_unsafe_schemes_cannot_execute() {
        let parsed = parse(
            "[PR **#228**](https://example.com/pr/228?x=1&amp;y=2) <https://example.com> ![alt](https://example.com/p.png) <b>text</b> [bad](javascript:evil) [file](file:///tmp/run)",
            MarkdownStyle::default(),
        );
        assert_eq!(parsed.links.len(), 2);
        assert_eq!(parsed.links[0].url, "https://example.com/pr/228?x=1&y=2");
        assert_eq!(&parsed.job.text[parsed.links[0].bytes.clone()], "PR #228");
        assert!(parsed.job.text.contains("alt <b>text</b> bad file"));
        assert_eq!(safe_url("https://example.com/\nanything"), None);
        assert_eq!(safe_url("data:text/html,evil"), None);
        assert_eq!(safe_url("mailto:me@example.com"), Some("mailto:me@example.com".into()));
    }

    #[test]
    fn wrapped_unicode_links_hit_only_visible_rows_and_return_an_intent() {
        let ctx = egui::Context::default();
        t::install(&ctx);
        let source = "Pré [a long café link that wraps across rows](https://example.com/path).";
        let mut click = None;
        let mut run = |events| {
            let mut rect = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(120.0, 200.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.set_width(120.0);
                    let parsed = parse(source, MarkdownStyle::default());
                    let mut job = parsed.job;
                    job.wrap.max_width = 120.0;
                    let galley = ui.painter().layout_job(job);
                    let link = &parsed.links[0];
                    let chars = galley.job.text[..link.bytes.start].chars().count()..galley.job.text[..link.bytes.end].chars().count();
                    let origin = ui.cursor().min;
                    let rects = link_rects(&galley, origin, chars);
                    assert!(rects.len() >= 2);
                    rect = rects.last().copied();
                    click = render(ui, source, MarkdownStyle::default(), 0.0).1;
                },
            );
            assert!(!output.platform_output.commands.iter().any(|command| matches!(command, egui::OutputCommand::OpenUrl(_))));
            output.textures_delta.clear();
            rect.unwrap()
        };
        let pos = run(Vec::new()).center();
        for pressed in [true, false] {
            run(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
            ]);
        }
        assert_eq!(click, Some("https://example.com/path".into()));
    }
}
