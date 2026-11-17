use tiny_skia::{IntSize, Pixmap};

use crate::windows::WindowId;

pub const LONGEST_SIDE: u32 = 1280;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channels {
    Rgba,
    Bgra,
}

pub struct Capture<'a> {
    pub bytes: &'a [u8],
    pub size: (u32, u32),
    pub channels: Channels,
    pub opaque: bool,
}

pub fn thumbnail(capture: &Capture, longest: u32) -> Option<Pixmap> {
    let (wide, tall) = (capture.size.0 as usize, capture.size.1 as usize);
    let pixels = capture
        .bytes
        .get(..wide * tall * 4)
        .filter(|pixels| !pixels.is_empty())?;
    let block = wide.max(tall).div_ceil(longest.max(1) as usize).max(1);
    let (small_wide, small_tall) = (wide / block, tall / block);
    let mut small = Vec::with_capacity(small_wide * small_tall * 4);
    let mut sums = vec![0u32; small_wide * 4];
    let weight = (block * block) as u32;

    for band in pixels.chunks_exact(wide * 4 * block).take(small_tall) {
        sums.fill(0);
        for row in band.chunks_exact(wide * 4) {
            for (sum, cell) in sums
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(row.chunks_exact(block * 4))
            {
                for pixel in cell.as_chunks::<4>().0 {
                    sum[0] += pixel[0] as u32;
                    sum[1] += pixel[1] as u32;
                    sum[2] += pixel[2] as u32;
                    sum[3] += pixel[3] as u32;
                }
            }
        }
        for sum in sums.as_chunks::<4>().0 {
            let [first, green, third, alpha] = sum.map(|channel| (channel / weight) as u8);
            let (red, blue) = match capture.channels {
                Channels::Rgba => (first, third),
                Channels::Bgra => (third, first),
            };
            small.extend([red, green, blue, if capture.opaque { 255 } else { alpha }]);
        }
    }
    Pixmap::from_vec(small, IntSize::from_wh(small_wide as u32, small_tall as u32)?)
}

pub fn worth_capturing(
    candidates: &[WindowId],
    selected: usize,
    focused: Option<WindowId>,
    has_a_picture: impl Fn(WindowId) -> bool,
) -> Vec<WindowId> {
    let mut nearest_first: Vec<(usize, WindowId)> = candidates
        .iter()
        .enumerate()
        .filter(|(_, window)| Some(**window) == focused || !has_a_picture(**window))
        .map(|(index, window)| (index.abs_diff(selected), *window))
        .collect();
    nearest_first.sort();
    nearest_first.into_iter().map(|(_, window)| window).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::probes::*;

    fn captured(bytes: &[u8], size: (u32, u32), channels: Channels, opaque: bool) -> Option<Pixmap> {
        thumbnail(
            &Capture {
                bytes,
                size,
                channels,
                opaque,
            },
            LONGEST_SIDE,
        )
    }

    #[test]
    fn a_capture_in_either_channel_order_keeps_its_colours() {
        let straight = captured(&[255, 128, 0, 255], (1, 1), Channels::Rgba, false).unwrap();
        let swapped = captured(&[0, 128, 255, 255], (1, 1), Channels::Bgra, false).unwrap();

        assert_eq!(rgba(&straight, 0.0, 0.0), [255, 128, 0, 255]);
        assert_eq!(rgba(&swapped, 0.0, 0.0), [255, 128, 0, 255]);
    }

    #[test]
    fn a_capture_without_an_alpha_channel_is_fully_opaque() {
        let image = captured(&[10, 20, 30, 0], (1, 1), Channels::Rgba, true).unwrap();

        assert_eq!(rgba(&image, 0.0, 0.0), [10, 20, 30, 255]);
    }

    #[test]
    fn a_capture_shorter_than_its_claimed_size_is_rejected() {
        assert!(captured(&[0; 12], (2, 2), Channels::Rgba, false).is_none());
        assert!(captured(&[], (0, 0), Channels::Rgba, false).is_none());
    }

    #[test]
    fn a_large_window_is_shrunk_to_the_longest_side_keeping_its_proportions() {
        let screen = vec![0u8; 3840 * 2160 * 4];

        let picture = captured(&screen, (3840, 2160), Channels::Rgba, true).unwrap();

        assert_eq!((picture.width(), picture.height()), (1280, 720));
    }

    #[test]
    fn a_window_that_already_fits_is_kept_pixel_for_pixel() {
        let window = [[9u8, 8, 7, 255]; 6].concat();

        let picture = captured(&window, (3, 2), Channels::Rgba, false).unwrap();

        assert_eq!((picture.width(), picture.height()), (3, 2));
        assert_eq!(rgba(&picture, 2.0, 1.0), [9, 8, 7, 255]);
    }

    #[test]
    fn fine_detail_blends_instead_of_turning_into_noise() {
        let stripes: Vec<u8> = (0..512 * 512)
            .flat_map(|index| [if index % 2 == 0 { 255 } else { 0 }; 4])
            .collect();

        let picture = thumbnail(
            &Capture {
                bytes: &stripes,
                size: (512, 512),
                channels: Channels::Rgba,
                opaque: true,
            },
            64,
        )
        .unwrap();

        assert_eq!((picture.width(), picture.height()), (64, 64));
        assert!(
            [3.0, 17.0, 40.0, 60.0]
                .iter()
                .all(|x| (126..=128).contains(&rgba(&picture, *x, 32.0)[0]))
        );
    }

    #[test]
    fn a_size_that_does_not_divide_evenly_drops_the_ragged_edge() {
        let window = vec![200u8; 7 * 5 * 4];

        let picture = thumbnail(
            &Capture {
                bytes: &window,
                size: (7, 5),
                channels: Channels::Rgba,
                opaque: true,
            },
            3,
        )
        .unwrap();

        assert_eq!((picture.width(), picture.height()), (2, 1));
        assert_eq!(rgba(&picture, 1.0, 0.0), [200, 200, 200, 255]);
    }

    fn ids(numbers: &[u32]) -> Vec<WindowId> {
        numbers.iter().map(|number| WindowId(*number)).collect()
    }

    #[test]
    fn only_windows_without_a_picture_and_the_one_in_use_need_capturing() {
        let pictured = ids(&[2, 3]);

        let wanted = worth_capturing(&ids(&[1, 2, 3, 4]), 1, Some(WindowId(3)), |window| {
            pictured.contains(&window)
        });

        assert_eq!(wanted, ids(&[1, 3, 4]));
    }

    #[test]
    fn the_windows_nearest_the_selection_are_captured_first() {
        let wanted = worth_capturing(&ids(&[1, 2, 3, 4, 5]), 3, None, |_| false);

        assert_eq!(wanted, ids(&[4, 3, 5, 2, 1]));
    }

    #[test]
    fn nothing_needs_capturing_when_every_picture_is_at_hand() {
        assert_eq!(worth_capturing(&ids(&[1, 2]), 0, None, |_| true), []);
    }
}
