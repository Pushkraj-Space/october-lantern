//! Artwork, decoded and uploaded once at startup; read-only afterwards (CONTRACT §11.8).
//! Artwork stays sourced from macos/Resources; there are no OS-specific copies.
use crate::model::AgentKind;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::collections::HashMap;

pub struct Assets {
    /// The lantern mascot (repo root logo.png).
    pub logo: TextureHandle,
    /// Swift's idle `.saturation(0.55)`; apply `.opacity(0.8)` when drawing.
    pub logo_idle: TextureHandle,
    pub october: TextureHandle,
    pub desktop_hero: TextureHandle,
    harnesses: HashMap<&'static str, TextureHandle>,
}

impl Assets {
    pub fn load(ctx: &egui::Context) -> Self {
        let mut harnesses = HashMap::new();
        macro_rules! harness {
            ($($kind:literal),+ $(,)?) => {$({
                harnesses.insert($kind, texture(ctx, concat!("harness/", $kind),
                    include_bytes!(concat!("../../../macos/Resources/harness/", $kind, ".png"))));
            })+};
        }
        harness!(
            "claude", "codex", "gemini", "opencode", "pi", "october", "grok", "cursor", "copilot", "qwen", "goose", "aider", "amp", "kimi",
            "droid", "crush", "auggie"
        );
        let logo_image = decode("logo", include_bytes!("../../../logo.png"));
        let logo = upload(ctx, "logo", &logo_image);
        let mut idle = logo_image;
        for pixel in idle.pixels_mut() {
            let [r, g, b, _] = pixel.0;
            let gray = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
            for channel in &mut pixel.0[..3] {
                *channel = (gray + (*channel as f32 - gray) * 0.55).round() as u8;
            }
        }
        Self {
            logo,
            logo_idle: upload(ctx, "logo/idle", &idle),
            october: texture(ctx, "october", include_bytes!("../../../macos/Resources/october.png")),
            desktop_hero: texture(ctx, "october/hero-palace", include_bytes!("../../../macos/Resources/october/hero-palace.jpg")),
            harnesses,
        }
    }

    pub fn harness(&self, kind: &AgentKind) -> Option<&TextureHandle> {
        self.harnesses.get(kind.as_str())
    }
}

/// Decodes bundled PNG/JPEG bytes into a texture. Bundled art always decodes; a failure is a build mistake.
pub fn texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> TextureHandle {
    upload(ctx, name, &decode(name, bytes))
}

fn decode(name: &str, bytes: &[u8]) -> image::RgbaImage {
    image::load_from_memory(bytes).unwrap_or_else(|error| panic!("bundled image {name}: {error}")).to_rgba8()
}

fn upload(ctx: &egui::Context, name: &str, image: &image::RgbaImage) -> TextureHandle {
    let size = [image.width() as usize, image.height() as usize];
    ctx.load_texture(name, ColorImage::from_rgba_unmultiplied(size, image.as_raw()), TextureOptions::LINEAR)
}
