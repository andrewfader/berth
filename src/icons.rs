use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use resvg::usvg;
use tiny_skia::{Pixmap, Transform};

use crate::geometry::Rect;
use crate::paint;
use crate::text::Typesetter;

pub struct Icons {
    theme: String,
    loaded: HashMap<(String, u32), Rc<Pixmap>>,
}

impl Icons {
    pub fn themed(theme: impl Into<String>) -> Self {
        Self {
            theme: theme.into(),
            loaded: HashMap::new(),
        }
    }

    pub fn icon(
        &mut self,
        name: Option<&str>,
        label: &str,
        size: u32,
        typesetter: &mut Typesetter,
    ) -> Rc<Pixmap> {
        let key = (name.unwrap_or(label).to_owned(), size);
        if let Some(icon) = self.loaded.get(&key) {
            return icon.clone();
        }
        let icon = name
            .and_then(|name| self.locate(name, size))
            .and_then(|path| rasterize(&path, size))
            .unwrap_or_else(|| monogram(label, size, typesetter));
        let icon = Rc::new(icon);
        self.loaded.insert(key, icon.clone());
        icon
    }

    fn locate(&self, name: &str, size: u32) -> Option<PathBuf> {
        if Path::new(name).is_absolute() {
            return Some(PathBuf::from(name));
        }
        freedesktop_icons::lookup(name)
            .with_size(size.min(u16::MAX as u32) as u16)
            .with_theme(&self.theme)
            .with_cache()
            .find()
    }
}

pub fn rasterize(path: &Path, size: u32) -> Option<Pixmap> {
    let data = fs::read(path).ok()?;
    let mut canvas = Pixmap::new(size, size)?;
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
    {
        let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;
        let fit = size as f32 / tree.size().width().max(tree.size().height());
        resvg::render(&tree, Transform::from_scale(fit, fit), &mut canvas.as_mut());
    } else {
        let image = Pixmap::decode_png(&data).ok()?;
        paint::picture(
            &mut canvas.as_mut(),
            &image,
            Rect::new(0.0, 0.0, size as f32, size as f32),
            0.0,
            1.0,
            1.0,
        );
    }
    Some(canvas)
}

pub fn monogram(label: &str, size: u32, typesetter: &mut Typesetter) -> Pixmap {
    let mut canvas = Pixmap::new(size.max(1), size.max(1)).expect("an icon has a size");
    let side = size as f32;
    let hue = label
        .bytes()
        .fold(0u32, |sum, byte| sum.wrapping_mul(31).wrapping_add(byte as u32));
    let [red, green, blue] = [hue, hue >> 8, hue >> 16].map(|channel| 70 + (channel % 110) as u8);
    paint::fill_rounded(
        &mut canvas.as_mut(),
        Rect::new(0.0, 0.0, side, side).grown(-side * 0.06),
        side * 0.22,
        &paint::tint(red, green, blue, 1.0),
        1.0,
    );
    let initial: String = label
        .chars()
        .find(|letter| letter.is_alphanumeric())
        .into_iter()
        .flat_map(char::to_uppercase)
        .collect();
    if let Some(letter) = typesetter.line(&initial, side * 0.5, side) {
        let (wide, tall) = (letter.width() as f32, letter.height() as f32);
        paint::picture(
            &mut canvas.as_mut(),
            &letter,
            Rect::new((side - wide) / 2.0, (side - tall) / 2.0, wide, tall),
            0.0,
            1.0,
            1.0,
        );
    }
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::probes::*;

    const BLUE_CIRCLE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16">
        <circle cx="8" cy="8" r="8" fill="#0000ff"/></svg>"##;

    fn saved(dir: &Path, name: &str, contents: &[u8]) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn an_svg_icon_is_drawn_sharp_at_the_size_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let path = saved(dir.path(), "circle.svg", BLUE_CIRCLE.as_bytes());

        let icon = rasterize(&path, 96).unwrap();

        assert_eq!((icon.width(), icon.height()), (96, 96));
        assert_eq!(rgba(&icon, 48.0, 48.0), [0, 0, 255, 255]);
        assert_eq!(rgba(&icon, 48.0, 2.0), [0, 0, 255, 255]);
        assert_eq!(alpha(&icon, 2.0, 2.0), 0);
    }

    #[test]
    fn a_png_icon_is_scaled_to_the_size_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let path = saved(dir.path(), "red.png", &solid(8, 255, 0, 0).encode_png().unwrap());

        let icon = rasterize(&path, 64).unwrap();

        assert_eq!((icon.width(), icon.height()), (64, 64));
        assert_eq!(rgba(&icon, 60.0, 60.0), [255, 0, 0, 255]);
    }

    #[test]
    fn a_missing_or_corrupt_file_is_not_an_icon() {
        let dir = tempfile::tempdir().unwrap();

        assert!(rasterize(&dir.path().join("absent.png"), 48).is_none());
        assert!(rasterize(&saved(dir.path(), "broken.png", b"not a png"), 48).is_none());
        assert!(rasterize(&saved(dir.path(), "broken.svg", b"<svg"), 48).is_none());
    }

    #[test]
    fn an_app_without_an_icon_gets_a_tile_with_its_initial() {
        let mut typesetter = Typesetter::default();
        let mut icons = Icons::themed("hicolor");

        let icon = icons.icon(None, "editor", 64, &mut typesetter);

        assert_eq!((icon.width(), icon.height()), (64, 64));
        assert_eq!(alpha(&icon, 32.0, 8.0), 255);
        assert!(
            icon.pixels()
                .iter()
                .any(|pixel| pixel.red() == 255 && pixel.green() == 255 && pixel.blue() == 255)
        );
    }

    #[test]
    fn apps_without_icons_are_told_apart_by_colour() {
        let mut typesetter = Typesetter::default();

        let editor = monogram("editor", 64, &mut typesetter);
        let browser = monogram("browser", 64, &mut typesetter);

        assert_ne!(rgba(&editor, 32.0, 8.0), rgba(&browser, 32.0, 8.0));
    }

    #[test]
    fn an_icon_given_as_a_path_is_used_directly_and_loaded_only_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = saved(dir.path(), "red.png", &solid(8, 255, 0, 0).encode_png().unwrap());
        let mut typesetter = Typesetter::default();
        let mut icons = Icons::themed("hicolor");

        let first = icons.icon(path.to_str(), "red", 32, &mut typesetter);
        fs::remove_file(&path).unwrap();
        let again = icons.icon(path.to_str(), "red", 32, &mut typesetter);

        assert_eq!(rgba(&first, 16.0, 16.0), [255, 0, 0, 255]);
        assert!(Rc::ptr_eq(&first, &again));
    }
}
