use crate::geometry::Rect;

pub const TITLE: f32 = 30.0;
const MARGIN: f32 = 32.0;
const GAP: f32 = 24.0;
const SEARCH: (f32, f32) = (440.0, 44.0);
const APP: (f32, f32) = (120.0, 132.0);
const WIDEST_CELL: f32 = 560.0;
const SHAPE: f32 = 1.6;

#[derive(Clone, Debug, PartialEq)]
pub struct Places {
    pub search: Rect,
    pub windows: Vec<Rect>,
    pub apps: Vec<Rect>,
    pub columns: usize,
}

impl Places {
    pub fn pick_at(&self, x: f32, y: f32) -> Option<usize> {
        self.windows
            .iter()
            .chain(&self.apps)
            .position(|place| place.contains(x, y))
    }
}

pub fn arrange(stage: (f32, f32), windows: usize, apps: usize) -> Places {
    let (wide, tall) = stage;
    let search = Rect::new((wide - SEARCH.0) / 2.0, MARGIN, SEARCH.0, SEARCH.1);
    let top = search.bottom() + GAP;
    let apps_top = if windows == 0 {
        (tall - APP.1) / 2.0
    } else {
        tall - MARGIN - APP.1
    };
    let bottom = if apps == 0 { tall - MARGIN } else { apps_top - GAP };
    let room = Rect::new(MARGIN, top, wide - 2.0 * MARGIN, (bottom - top).max(1.0));

    let cell_for = |columns: usize| {
        let rows = windows.div_ceil(columns);
        let across = (room.width - GAP * (columns as f32 - 1.0)) / columns as f32;
        let down = (room.height - GAP * (rows as f32 - 1.0)) / rows as f32;
        across.min((down - TITLE) * SHAPE).min(WIDEST_CELL)
    };
    let columns = (1..=windows.max(1))
        .max_by(|a, b| cell_for(*a).total_cmp(&cell_for(*b)))
        .unwrap_or(1);
    let cell = (
        cell_for(columns).max(1.0),
        cell_for(columns).max(1.0) / SHAPE + TITLE,
    );
    let rows = windows.div_ceil(columns);
    let grid_tall = rows as f32 * cell.1 + (rows as f32 - 1.0).max(0.0) * GAP;
    let grid_top = room.y + (room.height - grid_tall).max(0.0) / 2.0;

    let windows = (0..windows)
        .map(|index| {
            let (row, column) = (index / columns, index % columns);
            let in_this_row = if row + 1 == rows {
                windows - row * columns
            } else {
                columns
            };
            let row_wide = in_this_row as f32 * cell.0 + (in_this_row as f32 - 1.0) * GAP;
            Rect::new(
                (wide - row_wide) / 2.0 + column as f32 * (cell.0 + GAP),
                grid_top + row as f32 * (cell.1 + GAP),
                cell.0,
                cell.1,
            )
        })
        .collect();
    let apps_wide = apps as f32 * APP.0 + (apps as f32 - 1.0).max(0.0) * GAP;
    let apps = (0..apps)
        .map(|index| {
            Rect::new(
                (wide - apps_wide) / 2.0 + index as f32 * (APP.0 + GAP),
                apps_top,
                APP.0,
                APP.1,
            )
        })
        .collect();

    Places {
        search,
        windows,
        apps,
        columns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAGE: (f32, f32) = (1920.0, 1080.0);
    const WHOLE: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };

    fn inside(place: &Rect, area: &Rect) -> bool {
        place.x >= area.x
            && place.y >= area.y
            && place.right() <= area.right()
            && place.bottom() <= area.bottom()
    }

    #[test]
    fn the_search_field_sits_at_the_top_in_the_middle() {
        let places = arrange(STAGE, 3, 0);

        assert_eq!(places.search.center_x(), 960.0);
        assert!(places.search.y < 60.0);
    }

    #[test]
    fn a_single_window_is_shown_large_in_the_middle() {
        let places = arrange(STAGE, 1, 0);

        assert_eq!(places.windows.len(), 1);
        assert_eq!(places.windows[0].center_x(), 960.0);
        assert!(places.windows[0].width > 500.0);
    }

    #[test]
    fn windows_fill_a_grid_without_overlapping_or_leaving_the_screen() {
        for count in [2, 3, 5, 8, 13, 30] {
            let places = arrange(STAGE, count, 0);

            assert_eq!(places.windows.len(), count);
            assert!(
                places.windows.iter().all(|place| inside(place, &WHOLE)),
                "{count} windows"
            );
            for (index, one) in places.windows.iter().enumerate() {
                assert!(
                    places.windows[index + 1..]
                        .iter()
                        .all(|other| !one.intersects(other)),
                    "{count} windows"
                );
                assert!(one.y > places.search.bottom());
            }
        }
    }

    #[test]
    fn more_windows_means_more_columns_and_smaller_pictures() {
        let (few, many) = (arrange(STAGE, 3, 0), arrange(STAGE, 12, 0));

        assert!(many.columns > few.columns);
        assert!(many.windows[0].width < few.windows[0].width);
    }

    #[test]
    fn a_short_last_row_is_centered() {
        let places = arrange(STAGE, 5, 0);
        let last_row: Vec<&Rect> = places
            .windows
            .iter()
            .filter(|place| place.y == places.windows[4].y)
            .collect();

        let middle = (last_row[0].x + last_row.last().unwrap().right()) / 2.0;
        assert!((middle - 960.0).abs() < 0.01);
    }

    #[test]
    fn matching_apps_line_up_in_a_row_under_the_windows() {
        let places = arrange(STAGE, 4, 3);

        assert_eq!(places.apps.len(), 3);
        assert!(
            places
                .apps
                .iter()
                .all(|app| places.windows.iter().all(|window| app.y >= window.bottom()))
        );
        assert_eq!((places.apps[0].x + places.apps[2].right()) / 2.0, 960.0);
        assert!(places.apps.iter().all(|place| inside(place, &WHOLE)));
    }

    #[test]
    fn with_no_window_left_the_apps_take_the_middle_of_the_screen() {
        let places = arrange(STAGE, 0, 2);

        assert!((places.apps[0].center_y() - 540.0).abs() < 1.0);
    }

    #[test]
    fn pointing_finds_the_window_or_app_under_the_pointer() {
        let places = arrange(STAGE, 2, 2);
        let (window, app) = (places.windows[1], places.apps[0]);

        assert_eq!(places.pick_at(window.center_x(), window.center_y()), Some(1));
        assert_eq!(places.pick_at(app.center_x(), app.center_y()), Some(2));
        assert_eq!(places.pick_at(4.0, 4.0), None);
    }
}
