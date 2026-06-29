//! Bundled screen backgrounds, decoded to egui textures at startup.
//!
//! The standby image and the per-category full-screen alert backgrounds are the
//! genuine artwork recovered from the legacy receiver; they drive the「リアル」
//! standby/alert styles. Files are stored under neutral names.

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use jalert_receiver::model::Category;

const STANDBY: &[u8] = include_bytes!("../../assets/screen/standby.jpg");
const CAT_PROTECT: &[u8] = include_bytes!("../../assets/screen/cat_protect.gif");
const CAT_EQ: &[u8] = include_bytes!("../../assets/screen/cat_eq.gif");
const CAT_TSUNAMI: &[u8] = include_bytes!("../../assets/screen/cat_tsunami.gif");
const CAT_VOLCANO: &[u8] = include_bytes!("../../assets/screen/cat_volcano.gif");

/// Decoded screen textures. Each may be `None` if decoding failed, in which case
/// the caller falls back to a procedural rendering.
pub struct Screens {
    pub standby: Option<TextureHandle>,
    protect: Option<TextureHandle>,
    eq: Option<TextureHandle>,
    tsunami: Option<TextureHandle>,
    volcano: Option<TextureHandle>,
}

fn tex(ctx: &Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let color = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    Some(ctx.load_texture(name, color, TextureOptions::LINEAR))
}

impl Screens {
    pub fn load(ctx: &Context) -> Self {
        Screens {
            standby: tex(ctx, "standby", STANDBY),
            protect: tex(ctx, "cat_protect", CAT_PROTECT),
            eq: tex(ctx, "cat_eq", CAT_EQ),
            tsunami: tex(ctx, "cat_tsunami", CAT_TSUNAMI),
            volcano: tex(ctx, "cat_volcano", CAT_VOLCANO),
        }
    }

    /// The real full-screen alert background for a category, if one exists.
    /// Weather has no dedicated artwork (it falls back to a colour-coded screen).
    pub fn category(&self, c: Category) -> Option<&TextureHandle> {
        match c {
            Category::CivilProtection => self.protect.as_ref(),
            Category::Eew | Category::Earthquake | Category::SeismicIntensity => self.eq.as_ref(),
            Category::Tsunami => self.tsunami.as_ref(),
            Category::Volcano => self.volcano.as_ref(),
            _ => None,
        }
    }
}
