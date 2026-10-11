//! Artwork, decoded and uploaded once at startup; read-only afterwards (CONTRACT §11.8).
//! Atlas extends this with the harness logos, October art and brand marks.
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

pub struct Assets {
    /// The lantern mascot (repo root logo.png).
    pub logo: TextureHandle,
}

impl Assets {
    pub fn load(ctx: &egui::Context) -> Self {
        Self { logo: texture(ctx, "logo", include_bytes!("../../../logo.png")) }
    }
}

/// Decodes bundled PNG/JPEG bytes into a texture. Bundled art always decodes; a failure is a build mistake.
pub fn texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> TextureHandle {
    let image = image::load_from_memory(bytes).unwrap_or_else(|error| panic!("bundled image {name}: {error}")).to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    ctx.load_texture(name, ColorImage::from_rgba_unmultiplied(size, image.as_raw()), TextureOptions::LINEAR)
}
