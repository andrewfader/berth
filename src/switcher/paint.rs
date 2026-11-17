use tiny_skia::{Pixmap, PixmapMut};

use super::carousel::Cover;
use crate::geometry::Rect;
use crate::paint::{Finish, fill_rounded, picture, picture_part, stroke_rounded, tint};

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
    pub finish: Finish,
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

pub fn uncovered(place: Rect, nearer: &[Rect]) -> Option<Rect> {
    let (mut left, mut right) = (place.x, place.right());
    for cover in nearer {
        if cover.y > place.y || cover.bottom() < place.bottom() {
            continue;
        }
        if cover.center_x() > place.center_x() {
            right = right.min(cover.x + CORNER + 2.0);
        } else {
            left = left.max(cover.right() - CORNER - 2.0);
        }
    }
    (right - left >= 1.0).then(|| Rect::new(left, place.y, right - left, place.height))
}

pub fn paint(canvas: &mut PixmapMut, scene: &Scene) {
    let scale = scene.scale;
    let mut floor: f32 = 0.0;
    let places: Vec<Option<Rect>> = scene
        .covers
        .iter()
        .map(|cover| Some(picture_frame(scene.cards.get(cover.index)?, cover.frame)))
        .collect();

    for (position, cover) in scene.covers.iter().enumerate() {
        let (Some(card), Some(place)) = (scene.cards.get(cover.index), places[position]) else {
            continue;
        };
        let chosen = cover.index == scene.selected;
        floor = floor.max(place.bottom());
        let in_front: Vec<Rect> = places[position + 1..].iter().flatten().copied().collect();
        let Some(seen) = uncovered(place, &in_front) else {
            continue;
        };

        if scene.finish == Finish::Fine {
            for (spread, shade) in [(9.0, 0.05), (5.0, 0.09), (2.0, 0.14)] {
                let rim = place.moved(0.0, 3.0);
                stroke_rounded(canvas, rim, CORNER, 2.0 * spread, &tint(0, 0, 0, shade), scale);
            }
        }
        match card.thumbnail {
            Some(thumbnail) => picture_part(canvas, thumbnail, place, seen, CORNER, scene.finish, scale),
            None => {
                fill_rounded(canvas, seen, CORNER, &tint(44, 44, 50, 1.0), scale);
                let side = (place.height * 0.4).min(128.0);
                let middle = Rect::new(
                    place.center_x() - side / 2.0,
                    place.center_y() - side / 2.0,
                    side,
                    side,
                );
                if middle.x >= seen.x && middle.right() <= seen.right() {
                    picture(canvas, card.icon, middle, 0.0, Finish::Fine, scale);
                }
            }
        }
        if cover.light < 1.0 {
            fill_rounded(
                canvas,
                seen,
                CORNER,
                &tint(0, 0, 0, (1.0 - cover.light) * 0.8),
                scale,
            );
        }
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
            if badge.x >= seen.x && badge.right() <= seen.right() {
                picture(canvas, card.icon, badge, 0.0, Finish::Fine, scale);
            }
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
            Finish::Fine,
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
        finished(thumbnails, selected, title, Finish::Fine, scale)
    }

    fn finished(
        thumbnails: &[Option<&Pixmap>],
        selected: usize,
        title: Option<&Pixmap>,
        finish: Finish,
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
                finish,
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
    fn a_soft_shadow_lifts_each_window_off_the_backdrop_once_the_row_is_at_rest() {
        let blue = screenshot(160, 100, 0, 0, 255);
        let front = picture_frame(
            &Card {
                thumbnail: Some(&blue),
                icon: &blue,
            },
            front_frame(STAGE),
        );
        let beside = (front.right() + 7.0, front.center_y());

        let at_rest = finished(&[Some(&blue)], 0, None, Finish::Fine, 1.0);
        let in_motion = finished(&[Some(&blue)], 0, None, Finish::Draft, 1.0);

        assert!(alpha(&at_rest, beside.0, beside.1) > 0);
        assert_eq!(alpha(&in_motion, beside.0, beside.1), 0);
        assert_eq!(
            rgba(&in_motion, front.center_x(), front.center_y()),
            [0, 0, 255, 255]
        );
    }

    #[test]
    fn only_the_strip_of_a_window_that_peeks_out_from_behind_the_others_is_worth_painting() {
        let behind = Rect::new(500.0, 100.0, 400.0, 300.0);
        let nearer_on_the_left = Rect::new(200.0, 50.0, 500.0, 350.0);

        let seen = uncovered(behind, &[nearer_on_the_left]).unwrap();

        assert_eq!((seen.right(), seen.y, seen.height), (900.0, 100.0, 300.0));
        assert!(seen.x > 650.0 && seen.x < 700.0);
    }

    #[test]
    fn a_window_taller_than_the_one_in_front_of_it_is_painted_whole() {
        let tall = Rect::new(500.0, 0.0, 400.0, 400.0);
        let short_in_front = Rect::new(200.0, 200.0, 500.0, 200.0);

        assert_eq!(uncovered(tall, &[short_in_front]), Some(tall));
    }

    #[test]
    fn a_window_completely_behind_its_neighbours_is_not_painted_at_all() {
        let buried = Rect::new(400.0, 100.0, 100.0, 100.0);
        let (left, right) = (
            Rect::new(100.0, 50.0, 380.0, 200.0),
            Rect::new(420.0, 50.0, 380.0, 200.0),
        );

        assert_eq!(uncovered(buried, &[left, right]), None);
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
