use tiny_skia::{FilterQuality, IntSize, Pixmap, PixmapPaint, Transform};

pub const LONGEST_SIDE: u32 = 1280;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channels {
    Rgba,
    Bgra,
}

pub fn from_capture(bytes: &[u8], size: (u32, u32), channels: Channels, opaque: bool) -> Option<Pixmap> {
    let mut pixels = bytes.get(..size.0 as usize * size.1 as usize * 4)?.to_vec();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        if channels == Channels::Bgra {
            pixel.swap(0, 2);
        }
        if opaque {
            pixel[3] = 255;
        }
    }
    Pixmap::from_vec(pixels, IntSize::from_wh(size.0, size.1)?)
}

pub fn shrink(image: Pixmap, longest: u32) -> Pixmap {
    let mut image = image;
    while image.width().max(image.height()) > longest {
        let ratio = (longest as f32 / image.width().max(image.height()) as f32).max(0.5);
        let wide = ((image.width() as f32 * ratio).round() as u32).max(1);
        let tall = ((image.height() as f32 * ratio).round() as u32).max(1);
        let Some(mut smaller) = Pixmap::new(wide, tall) else {
            break;
        };
        smaller.draw_pixmap(
            0,
            0,
            image.as_ref(),
            &PixmapPaint {
                quality: FilterQuality::Bilinear,
                ..PixmapPaint::default()
            },
            Transform::from_scale(
                wide as f32 / image.width() as f32,
                tall as f32 / image.height() as f32,
            ),
            None,
        );
        image = smaller;
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::probes::*;

    #[test]
    fn a_capture_in_either_channel_order_keeps_its_colours() {
        let orange_rgba = [255, 128, 0, 255];
        let orange_bgra = [0, 128, 255, 255];

        let straight = from_capture(&orange_rgba, (1, 1), Channels::Rgba, false).unwrap();
        let swapped = from_capture(&orange_bgra, (1, 1), Channels::Bgra, false).unwrap();

        assert_eq!(rgba(&straight, 0.0, 0.0), [255, 128, 0, 255]);
        assert_eq!(rgba(&swapped, 0.0, 0.0), [255, 128, 0, 255]);
    }

    #[test]
    fn a_capture_without_an_alpha_channel_is_fully_opaque() {
        let padded = [10, 20, 30, 0];

        let image = from_capture(&padded, (1, 1), Channels::Rgba, true).unwrap();

        assert_eq!(rgba(&image, 0.0, 0.0), [10, 20, 30, 255]);
    }

    #[test]
    fn a_capture_shorter_than_its_claimed_size_is_rejected() {
        assert!(from_capture(&[0; 12], (2, 2), Channels::Rgba, false).is_none());
        assert!(from_capture(&[], (0, 0), Channels::Rgba, false).is_none());
    }

    #[test]
    fn a_large_window_is_shrunk_to_the_longest_side_keeping_its_proportions() {
        let thumbnail = shrink(Pixmap::new(3840, 2160).unwrap(), 1280);

        assert_eq!((thumbnail.width(), thumbnail.height()), (1280, 720));
    }

    #[test]
    fn a_window_that_already_fits_is_left_untouched() {
        let thumbnail = shrink(Pixmap::new(800, 600).unwrap(), 1280);

        assert_eq!((thumbnail.width(), thumbnail.height()), (800, 600));
    }

    #[test]
    fn fine_detail_blends_instead_of_turning_into_noise() {
        let mut stripes = Pixmap::new(512, 512).unwrap();
        for (index, pixel) in stripes.pixels_mut().iter_mut().enumerate() {
            let shade = if index % 2 == 0 { 255 } else { 0 };
            *pixel = tiny_skia::ColorU8::from_rgba(shade, shade, shade, 255).premultiply();
        }

        let thumbnail = shrink(stripes, 64);

        for x in [3.0, 17.0, 40.0, 60.0] {
            let grey = rgba(&thumbnail, x, 32.0)[0];
            assert!((110..=145).contains(&grey), "{grey}");
        }
    }
}
