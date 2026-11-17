use tiny_skia::{Pixmap, PixmapMut};

use super::carousel::Cover;
use crate::geometry::Rect;
use crate::paint::{fill_rounded, picture, stroke_rounded, tint};

const CORNER: f32 = 10.0;
const BADGE: f32 = 44.0;

pub struct Card<'a> {
    pub thumbnail: Option<&'a Pixmap>,
    pub icon: &'a Pixmap,
}

pub struct Scene<'a> {
    pub covers: &'a [Cover],
    pub cards: &'a [Card<'a>],
    pub selected: usize,
    pub title: Option<&'a Pixmap>,
    pub scale: f32,
}

pub fn picture_frame(card: &Card, frame: Rect) -> Rect {
    let aspect = card.thumbnail.map_or(1.6, |thumbnail| {
        thumbnail.width() as f32 / thumbnail.height() as f32
    });
    let (wide, tall) = if aspect > frame.width / frame.height {
        (frame.width, frame.width / aspect)
    } else {
        (frame.height * aspect, frame.height)
    };
    Rect::new(frame.center_x() - wide / 2.0, frame.bottom() - tall, wide, tall)
}

pub fn paint(canvas: &mut PixmapMut, scene: &Scene) {
    let scale = scene.scale;
    let mut floor: f32 = 0.0;

    for cover in scene.covers {
        let Some(card) = scene.cards.get(cover.index) else {
            continue;
        };
        let place = picture_frame(card, cover.frame);
        let chosen = cover.index == scene.selected;
        floor = floor.max(place.bottom());

        for (spread, shade) in [(9.0, 0.05), (5.0, 0.09), (2.0, 0.14)] {
            fill_rounded(
                canvas,
                place.grown(spread).moved(0.0, 3.0),
                CORNER + spread,
                &tint(0, 0, 0, shade),
                scale,
            );
        }
        match card.thumbnail {
            Some(thumbnail) => picture(canvas, thumbnail, place, CORNER, 1.0, scale),
            None => {
                fill_rounded(canvas, place, CORNER, &tint(44, 44, 50, 1.0), scale);
                let side = (place.height * 0.4).min(128.0);
                let middle = Rect::new(
                    place.center_x() - side / 2.0,
                    place.center_y() - side / 2.0,
                    side,
                    side,
                );
                picture(canvas, card.icon, middle, 0.0, 1.0, scale);
            }
        }
        fill_rounded(
            canvas,
            place,
            CORNER,
            &tint(0, 0, 0, (1.0 - cover.light) * 0.8),
            scale,
        );
        if chosen {
            stroke_rounded(
                canvas,
                place.grown(2.0),
                CORNER + 2.0,
                3.0,
                &tint(255, 255, 255, 0.92),
                scale,
            );
        }
        if card.thumbnail.is_some() {
            let side =
                BADGE * place.height / cover.frame.height.max(place.height) * (0.6 + 0.4 * cover.light);
            let badge = Rect::new(
                place.center_x() - side / 2.0,
                place.bottom() - side * 0.62,
                side,
                side,
            );
            picture(canvas, card.icon, badge, 0.0, 1.0, scale);
        }
    }

    if let Some(title) = scene.title {
        let (wide, tall) = (title.width() as f32 / scale, title.height() as f32 / scale);
        let stage_width = canvas.width() as f32 / scale;
        picture(
            canvas,
            title,
            Rect::new((stage_width - wide) / 2.0, floor + BADGE * 0.6, wide, tall),
            0.0,
            1.0,
            scale,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::probes::*;
    use crate::switcher::carousel::{arrange, front_frame};
    use tiny_skia::Color;

    const STAGE: (f32, f32) = (1600.0, 600.0);

    fn screenshot(wide: u32, tall: u32, red: u8, green: u8, blue: u8) -> Pixmap {
        let mut pixmap = Pixmap::new(wide, tall).unwrap();
        pixmap.fill(Color::from_rgba8(red, green, blue, 255));
        pixmap
    }

    fn painted(
        thumbnails: &[Option<&Pixmap>],
        selected: usize,
        title: Option<&Pixmap>,
        scale: f32,
    ) -> Pixmap {
        let icon = solid(64, 255, 255, 0);
        let cards: Vec<Card> = thumbnails
            .iter()
            .map(|thumbnail| Card {
                thumbnail: *thumbnail,
                icon: &icon,
            })
            .collect();
        let covers = arrange(cards.len(), selected as f32, STAGE);
        let mut pixmap = Pixmap::new((STAGE.0 * scale) as u32, (STAGE.1 * scale) as u32).unwrap();
        paint(
            &mut pixmap.as_mut(),
            &Scene {
                covers: &covers,
                cards: &cards,
                selected,
                title,
                scale,
            },
        );
        pixmap
    }

    #[test]
    fn the_selected_window_is_shown_in_full_colour_at_center_stage() {
        let (red, blue) = (screenshot(160, 100, 255, 0, 0), screenshot(160, 100, 0, 0, 255));

        let stage = painted(&[Some(&red), Some(&blue), Some(&red)], 1, None, 1.0);

        let front = front_frame(STAGE);
        assert_eq!(rgba(&stage, front.center_x(), front.center_y()), [0, 0, 255, 255]);
    }

    #[test]
    fn its_neighbours_stand_behind_it_dimmed() {
        let (red, blue) = (screenshot(160, 100, 255, 0, 0), screenshot(160, 100, 0, 0, 255));

        let stage = painted(&[Some(&red), Some(&blue), Some(&red)], 1, None, 1.0);

        let front = front_frame(STAGE);
        let [beside_red, _, beside_blue, _] = rgba(&stage, front.right() + 60.0, front.bottom() - 40.0);
        assert!(
            beside_red > 100 && beside_red < 240 && beside_blue == 0,
            "{beside_red} {beside_blue}"
        );
        assert_eq!(
            rgba(&stage, front.right() - 20.0, front.center_y()),
            [0, 0, 255, 255]
        );
    }

    #[test]
    fn a_frame_marks_the_selected_window() {
        let blue = screenshot(160, 100, 0, 0, 255);

        let stage = painted(&[Some(&blue)], 0, None, 1.0);

        let front = picture_frame(
            &Card {
                thumbnail: Some(&blue),
                icon: &blue,
            },
            front_frame(STAGE),
        );
        assert!(
            rgba(&stage, front.center_x(), front.y - 2.0)
                .iter()
                .all(|channel| *channel > 230)
        );
        assert!(rgba(&stage, front.center_x(), front.y - 8.0)[0] < 30);
    }

    #[test]
    fn a_window_keeps_its_proportions_inside_its_frame() {
        let tall_window = screenshot(100, 400, 0, 255, 0);
        let front = front_frame(STAGE);

        let place = picture_frame(
            &Card {
                thumbnail: Some(&tall_window),
                icon: &tall_window,
            },
            front,
        );

        assert_eq!(place.height, front.height);
        assert!((place.width - front.height / 4.0).abs() < 0.01);
        assert_eq!(
            (place.center_x(), place.bottom()),
            (front.center_x(), front.bottom())
        );
    }

    #[test]
    fn each_window_carries_the_icon_of_its_app() {
        let blue = screenshot(160, 100, 0, 0, 255);

        let stage = painted(&[Some(&blue)], 0, None, 1.0);

        let front = front_frame(STAGE);
        assert_eq!(
            rgba(&stage, front.center_x(), front.bottom() - 4.0),
            [255, 255, 0, 255]
        );
    }

    #[test]
    fn a_window_that_could_not_be_captured_is_shown_as_a_card_with_its_icon() {
        let stage = painted(&[None], 0, None, 1.0);

        let front = front_frame(STAGE);
        assert_eq!(
            rgba(&stage, front.center_x(), front.center_y()),
            [255, 255, 0, 255]
        );
        assert_eq!(rgba(&stage, front.center_x(), front.y + 12.0), [44, 44, 50, 255]);
    }

    #[test]
    fn the_title_of_the_selected_window_is_written_under_the_row() {
        let blue = screenshot(160, 100, 0, 0, 255);
        let title = screenshot(200, 20, 0, 255, 0);

        let stage = painted(&[Some(&blue)], 0, Some(&title), 1.0);

        let front = front_frame(STAGE);
        assert_eq!(
            rgba(&stage, 800.0, front.bottom() + BADGE * 0.6 + 10.0),
            [0, 255, 0, 255]
        );
    }

    #[test]
    fn the_space_around_the_windows_stays_clear_for_the_backdrop() {
        let blue = screenshot(160, 100, 0, 0, 255);

        let stage = painted(&[Some(&blue)], 0, None, 1.0);

        assert_eq!(alpha(&stage, 20.0, 20.0), 0);
        assert_eq!(alpha(&stage, 1580.0, 580.0), 0);
    }

    #[test]
    fn on_a_dense_display_the_stage_is_drawn_at_double_resolution() {
        let blue = screenshot(160, 100, 0, 0, 255);

        let stage = painted(&[Some(&blue)], 0, None, 2.0);

        let front = front_frame(STAGE);
        assert_eq!((stage.width(), stage.height()), (3200, 1200));
        assert_eq!(
            rgba(&stage, front.center_x() * 2.0, front.center_y() * 2.0),
            [0, 0, 255, 255]
        );
    }
}
