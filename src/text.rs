use cosmic_text::{
    Attrs, Buffer, Color, Ellipsize, EllipsizeHeightLimit, FontSystem, Metrics, Shaping, SwashCache, Wrap,
};
use tiny_skia::{ColorU8, Pixmap};

pub struct Typesetter {
    fonts: FontSystem,
    glyphs: SwashCache,
}

impl Default for Typesetter {
    fn default() -> Self {
        Self {
            fonts: FontSystem::new(),
            glyphs: SwashCache::new(),
        }
    }
}

impl Typesetter {
    pub fn line(&mut self, text: &str, size: f32, widest: f32) -> Option<Pixmap> {
        let height = (size * 1.4).ceil();
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(size, height));
        buffer.set_wrap(Wrap::None);
        buffer.set_ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)));
        buffer.set_size(Some(widest), Some(height));
        buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.fonts, false);

        let width = buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0, f32::max)
            .ceil()
            .min(widest);
        let mut pixmap = Pixmap::new(width as u32, height as u32)?;
        let (columns, rows) = (pixmap.width() as i32, pixmap.height() as i32);
        let pixels = pixmap.pixels_mut();
        buffer.draw(
            &mut self.fonts,
            &mut self.glyphs,
            Color::rgb(255, 255, 255),
            |x, y, wide, tall, color| {
                for row in y..y + tall as i32 {
                    for column in x..x + wide as i32 {
                        if (0..columns).contains(&column) && (0..rows).contains(&row) {
                            let at = (row * columns + column) as usize;
                            let ink =
                                ColorU8::from_rgba(color.r(), color.g(), color.b(), color.a()).premultiply();
                            if ink.alpha() > pixels[at].alpha() {
                                pixels[at] = ink;
                            }
                        }
                    }
                }
            },
        );
        Some(pixmap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inked(pixmap: &Pixmap) -> usize {
        pixmap.pixels().iter().filter(|pixel| pixel.alpha() > 0).count()
    }

    #[test]
    fn a_line_of_text_is_drawn_no_larger_than_it_needs() {
        let mut typesetter = Typesetter::default();

        let short = typesetter.line("Files", 14.0, 600.0).unwrap();
        let longer = typesetter.line("Files and folders", 14.0, 600.0).unwrap();

        assert!(inked(&short) > 0);
        assert!(short.width() < longer.width() && longer.width() < 600);
        assert_eq!(short.height(), 20);
    }

    #[test]
    fn a_title_too_long_for_its_space_is_cut_to_fit() {
        let mut typesetter = Typesetter::default();

        let title = typesetter
            .line(&"an endless window title ".repeat(20), 14.0, 200.0)
            .unwrap();

        assert!(title.width() <= 200);
        assert!(inked(&title) > 0);
    }

    #[test]
    fn larger_text_takes_more_room() {
        let mut typesetter = Typesetter::default();

        let small = typesetter.line("Terminal", 12.0, 600.0).unwrap();
        let large = typesetter.line("Terminal", 24.0, 600.0).unwrap();

        assert!(large.width() > small.width() && large.height() > small.height());
    }

    #[test]
    fn there_is_nothing_to_draw_for_an_empty_string() {
        assert!(Typesetter::default().line("", 14.0, 600.0).is_none());
    }
}
