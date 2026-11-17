use tiny_skia::{Color, Pixmap, PixmapMut};

use super::layout::{Places, TITLE};
use crate::geometry::Rect;
use crate::paint::{Finish, fill_rounded, picture, stroke_rounded, tint};

const CORNER: f32 = 10.0;
const ICON: f32 = 72.0;

pub struct Card<'a> {
    pub thumbnail: Option<&'a Pixmap>,
    pub icon: &'a Pixmap,
    pub title: Option<&'a Pixmap>,
    pub number: Option<&'a Pixmap>,
}

pub struct Launcher<'a> {
    pub icon: &'a Pixmap,
    pub name: Option<&'a Pixmap>,
}

pub struct Scene<'a> {
    pub places: &'a Places,
    pub windows: &'a [Card<'a>],
    pub apps: &'a [Launcher<'a>],
    pub selected: Option<usize>,
    pub search: Option<&'a Pixmap>,
    pub dimness: f32,
    pub scale: f32,
}

pub fn picture_place(card: &Card, cell: Rect) -> Rect {
    let room = Rect::new(cell.x, cell.y, cell.width, cell.height - TITLE);
    let shape = card
        .thumbnail
        .map_or(1.6, |picture| picture.width() as f32 / picture.height() as f32);
    let (wide, tall) = if shape > room.width / room.height {
        (room.width, room.width / shape)
    } else {
        (room.height * shape, room.height)
    };
    Rect::new(room.center_x() - wide / 2.0, room.bottom() - tall, wide, tall)
}

fn centered(canvas: &mut PixmapMut, text: &Pixmap, around: f32, top: f32, scale: f32) {
    let (wide, tall) = (text.width() as f32 / scale, text.height() as f32 / scale);
    picture(
        canvas,
        text,
        Rect::new(around - wide / 2.0, top, wide, tall),
        0.0,
        Finish::Fine,
        scale,
    );
}

pub fn paint(canvas: &mut PixmapMut, scene: &Scene) {
    let scale = scene.scale;
    canvas.fill(Color::from_rgba(0.0, 0.0, 0.0, scene.dimness.clamp(0.0, 1.0)).unwrap_or(Color::BLACK));

    let search = scene.places.search;
    fill_rounded(canvas, search, search.height / 2.0, &tint(24, 24, 28, 0.9), scale);
    stroke_rounded(
        canvas,
        search.grown(-0.5),
        search.height / 2.0,
        1.0,
        &tint(255, 255, 255, 0.2),
        scale,
    );
    if let Some(text) = scene.search {
        let tall = text.height() as f32 / scale;
        centered(
            canvas,
            text,
            search.center_x(),
            search.center_y() - tall / 2.0,
            scale,
        );
    }

    for (index, (card, cell)) in scene.windows.iter().zip(&scene.places.windows).enumerate() {
        let place = picture_place(card, *cell);
        match card.thumbnail {
            Some(thumbnail) => picture(canvas, thumbnail, place, CORNER, Finish::Fine, scale),
            None => {
                fill_rounded(canvas, place, CORNER, &tint(44, 44, 50, 1.0), scale);
                let side = (place.height * 0.4).min(128.0);
                let middle = Rect::new(
                    place.center_x() - side / 2.0,
                    place.center_y() - side / 2.0,
                    side,
                    side,
                );
                picture(canvas, card.icon, middle, 0.0, Finish::Fine, scale);
            }
        }
        if scene.selected == Some(index) {
            stroke_rounded(
                canvas,
                place.grown(2.0),
                CORNER + 2.0,
                3.0,
                &tint(255, 255, 255, 0.92),
                scale,
            );
        }
        if let Some(title) = card.title {
            centered(canvas, title, cell.center_x(), cell.bottom() - TITLE + 6.0, scale);
        }
        if let Some(number) = card.number {
            let tag = Rect::new(place.x + 8.0, place.y + 8.0, 26.0, 26.0);
            let tall = number.height() as f32 / scale;
            fill_rounded(canvas, tag, 13.0, &tint(24, 24, 28, 0.85), scale);
            centered(canvas, number, tag.center_x(), tag.center_y() - tall / 2.0, scale);
        }
    }

    for (index, (app, cell)) in scene.apps.iter().zip(&scene.places.apps).enumerate() {
        if scene.selected == Some(scene.windows.len() + index) {
            fill_rounded(canvas, *cell, CORNER, &tint(255, 255, 255, 0.16), scale);
        }
        let icon = Rect::new(cell.center_x() - ICON / 2.0, cell.y + 12.0, ICON, ICON);
        picture(canvas, app.icon, icon, 0.0, Finish::Fine, scale);
        if let Some(name) = app.name {
            centered(canvas, name, cell.center_x(), icon.bottom() + 10.0, scale);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overview::layout::arrange;
    use crate::paint::probes::*;

    const STAGE: (f32, f32) = (1600.0, 900.0);

    fn shot(red: u8, green: u8, blue: u8) -> Pixmap {
        let mut pixmap = Pixmap::new(160, 100).unwrap();
        pixmap.fill(Color::from_rgba8(red, green, blue, 255));
        pixmap
    }

    struct Painted {
        pixmap: Pixmap,
        places: Places,
    }

    fn painted(
        thumbnails: &[Option<&Pixmap>],
        apps: usize,
        selected: Option<usize>,
        search: bool,
    ) -> Painted {
        let (icon, text) = (solid(64, 255, 255, 0), solid(16, 0, 255, 255));
        let windows: Vec<Card> = thumbnails
            .iter()
            .map(|thumbnail| Card {
                thumbnail: *thumbnail,
                icon: &icon,
                title: Some(&text),
                number: None,
            })
            .collect();
        let launchers: Vec<Launcher> = (0..apps)
            .map(|_| Launcher {
                icon: &icon,
                name: Some(&text),
            })
            .collect();
        let places = arrange(STAGE, windows.len(), apps);
        let mut pixmap = Pixmap::new(STAGE.0 as u32, STAGE.1 as u32).unwrap();
        paint(
            &mut pixmap.as_mut(),
            &Scene {
                places: &places,
                windows: &windows,
                apps: &launchers,
                selected,
                search: search.then_some(&text),
                dimness: 0.5,
                scale: 1.0,
            },
        );
        Painted { pixmap, places }
    }

    #[test]
    fn the_desktop_behind_is_dimmed_everywhere() {
        let overview = painted(&[], 0, None, false);

        assert_eq!(alpha(&overview.pixmap, 5.0, 5.0), 128);
        assert_eq!(alpha(&overview.pixmap, 1595.0, 895.0), 128);
    }

    #[test]
    fn every_window_is_shown_in_its_cell_with_its_title_underneath() {
        let (red, blue) = (shot(255, 0, 0), shot(0, 0, 255));

        let overview = painted(&[Some(&red), Some(&blue)], 0, None, false);

        let cells = &overview.places.windows;
        assert_eq!(
            rgba(&overview.pixmap, cells[0].center_x(), cells[0].center_y() - 20.0),
            [255, 0, 0, 255]
        );
        assert_eq!(
            rgba(&overview.pixmap, cells[1].center_x(), cells[1].center_y() - 20.0),
            [0, 0, 255, 255]
        );
        assert_eq!(
            rgba(&overview.pixmap, cells[0].center_x(), cells[0].bottom() - 12.0),
            [0, 255, 255, 255]
        );
    }

    #[test]
    fn a_frame_marks_the_selected_window_only() {
        let red = shot(255, 0, 0);

        let overview = painted(&[Some(&red), Some(&red)], 0, Some(1), false);

        let framed = |index: usize| {
            let card = Card {
                thumbnail: Some(&red),
                icon: &red,
                title: None,
                number: None,
            };
            let place = picture_place(&card, overview.places.windows[index]);
            rgba(&overview.pixmap, place.center_x(), place.y - 2.0)[1] > 200
        };
        assert!(framed(1) && !framed(0));
    }

    #[test]
    fn a_window_can_carry_the_number_that_reaches_it_from_the_keyboard() {
        let (red, digit, icon) = (shot(255, 0, 0), solid(10, 0, 255, 255), solid(64, 255, 255, 0));
        let cards = [Card {
            thumbnail: Some(&red),
            icon: &icon,
            title: None,
            number: Some(&digit),
        }];
        let places = arrange(STAGE, 1, 0);
        let mut pixmap = Pixmap::new(STAGE.0 as u32, STAGE.1 as u32).unwrap();

        paint(
            &mut pixmap.as_mut(),
            &Scene {
                places: &places,
                windows: &cards,
                apps: &[],
                selected: None,
                search: None,
                dimness: 0.5,
                scale: 1.0,
            },
        );

        let place = picture_place(&cards[0], places.windows[0]);
        assert_eq!(rgba(&pixmap, place.x + 21.0, place.y + 21.0), [0, 255, 255, 255]);
        assert_eq!(rgba(&pixmap, place.x + 60.0, place.y + 21.0), [255, 0, 0, 255]);
    }

    #[test]
    fn a_window_without_a_picture_is_a_card_with_its_icon() {
        let overview = painted(&[None], 0, None, false);

        let card = Card {
            thumbnail: None,
            icon: &shot(0, 0, 0),
            title: None,
            number: None,
        };
        let place = picture_place(&card, overview.places.windows[0]);
        assert_eq!(
            rgba(&overview.pixmap, place.center_x(), place.center_y()),
            [255, 255, 0, 255]
        );
        assert_eq!(
            rgba(&overview.pixmap, place.x + 20.0, place.y + 20.0),
            [44, 44, 50, 255]
        );
    }

    #[test]
    fn matching_apps_are_drawn_as_icons_with_names_and_the_selected_one_is_lit() {
        let overview = painted(&[], 2, Some(1), false);

        let (first, second) = (overview.places.apps[0], overview.places.apps[1]);
        assert_eq!(
            rgba(&overview.pixmap, first.center_x(), first.y + 40.0),
            [255, 255, 0, 255]
        );
        assert_eq!(
            rgba(&overview.pixmap, first.center_x(), first.y + 12.0 + ICON + 18.0),
            [0, 255, 255, 255]
        );
        assert!(
            rgba(&overview.pixmap, second.x + 6.0, second.y + 60.0)[0]
                > rgba(&overview.pixmap, first.x + 6.0, first.y + 60.0)[0]
        );
    }

    #[test]
    fn what_was_typed_is_written_in_the_search_field() {
        let (empty, typed) = (painted(&[], 0, None, false), painted(&[], 0, None, true));
        let field = typed.places.search;

        assert_eq!(
            rgba(&typed.pixmap, field.center_x(), field.center_y()),
            [0, 255, 255, 255]
        );
        assert_ne!(
            rgba(&empty.pixmap, field.center_x(), field.center_y()),
            [0, 255, 255, 255]
        );
    }

    #[test]
    fn a_window_keeps_its_proportions_and_stands_on_its_title() {
        let tall = Pixmap::new(100, 400).unwrap();
        let cell = Rect::new(0.0, 0.0, 320.0, 230.0);

        let place = picture_place(
            &Card {
                thumbnail: Some(&tall),
                icon: &tall,
                title: None,
                number: None,
            },
            cell,
        );

        assert_eq!((place.height, place.width), (200.0, 50.0));
        assert_eq!((place.center_x(), place.bottom()), (160.0, 200.0));
    }
}
