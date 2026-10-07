//! Icons rendered from the SVG logo.

use anyhow::{Context, Result};
use resvg::{tiny_skia, usvg};

pub const LOGO_SVG: &str = include_str!("../assets/logo.svg");
pub const APP_ICON_SVG: &str = include_str!("../assets/app-icon.svg");

/// Menu bar icon size in pixels (18pt @2x).
pub const TRAY_SIZE: u32 = 36;

/// Renders `svg` into a `size`×`size` pixmap. `crop` is the square region of the
/// SVG's coordinate space to show (x, y, side); `None` shows the whole canvas.
fn render(svg: &str, size: u32, crop: Option<(f32, f32, f32)>) -> Result<tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default())?;
    let (x, y, side) = crop.unwrap_or((0.0, 0.0, tree.size().width().max(tree.size().height())));
    let scale = size as f32 / side;
    let mut pixmap = tiny_skia::Pixmap::new(size, size).context("pixmap")?;
    let transform = tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, -x * scale, -y * scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Ok(pixmap)
}

/// Black-on-transparent version of the logo for a macOS template image:
/// only the alpha channel matters, macOS tints it for light/dark menu bars.
pub fn tray_rgba() -> Result<Vec<u8>> {
    // The mark spans roughly x 104..394, y 72..440 in the 512 canvas.
    let pixmap = render(LOGO_SVG, TRAY_SIZE, Some((55.0, 62.0, 388.0)))?;
    Ok(pixmap
        .pixels()
        .iter()
        .flat_map(|p| [0, 0, 0, p.alpha()])
        .collect())
}

/// Writes the app icon as PNG (used to build the .icns in scripts/bundle.sh).
pub fn write_app_icon_png(size: u32, path: &std::path::Path) -> Result<()> {
    render(APP_ICON_SVG, size, None)?
        .save_png(path)
        .context("writing png")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn renders_tray_icon() {
        let rgba = super::tray_rgba().unwrap();
        assert_eq!(
            rgba.len(),
            (super::TRAY_SIZE * super::TRAY_SIZE * 4) as usize
        );
        let opaque = rgba.chunks(4).filter(|p| p[3] > 128).count();
        assert!(
            opaque > 200,
            "icon should have visible pixels, got {opaque}"
        );
    }
}
