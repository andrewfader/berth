use tiny_skia::{Pixmap, PixmapMut};

use super::actions::Effect;
use super::items::DockItem;
use crate::geometry::Rect;
use crate::paint::{Finish, fill_rounded, picture, stroke_rounded, tint};
use crate::windows::Windows;

pub const ROW: f32 = 30.0;
pub const MARGIN: f32 = 6.0;
pub const INSET: f32 = 12.0;
pub const WIDEST: f32 = 340.0;
const NARROWEST: f32 = 150.0;
const CORNER: f32 = 10.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    pub effects: Vec<Effect>,
}

pub fn entries(item: &DockItem, windows: &Windows, pins_are_editable: bool) -> Vec<Entry> {
    let mut entries: Vec<Entry> = item
        .oldest_first
        .iter()
        .filter_map(|id| windows.get(*id))
        .map(|window| Entry {
            label: window.title.clone(),
            effects: vec![Effect::Activate(window.id)],
        })
        .collect();
    entries.push(Entry {
        label: if item.is_running() { "New Window" } else { "Open" }.to_owned(),
        effects: vec![Effect::Launch(item.app.clone())],
    });
    if pins_are_editable {
        entries.push(match item.pinned {
            true => Entry {
                label: "Unpin from Dock".to_owned(),
                effects: vec![Effect::Unpin(item.app.clone())],
            },
            false => Entry {
                label: "Pin to Dock".to_owned(),
                effects: vec![Effect::Pin(item.app.clone())],
            },
        });
    }
    if item.is_running() {
        entries.push(Entry {
            label: "Quit".to_owned(),
            effects: item.oldest_first.iter().map(|id| Effect::Close(*id)).collect(),
        });
    }
    entries
}

pub fn size(rows: usize, widest_label: f32) -> (f32, f32) {
    let wide = (widest_label + 2.0 * INSET + 2.0 * MARGIN).clamp(NARROWEST, WIDEST);
    (wide.ceil(), rows as f32 * ROW + 2.0 * MARGIN)
}

pub fn row_at(rows: usize, y: f32) -> Option<usize> {
    let row = ((y - MARGIN) / ROW).floor();
    (y >= MARGIN && (row as usize) < rows).then_some(row as usize)
}

pub struct Scene<'a> {
    pub labels: &'a [Option<&'a Pixmap>],
    pub pointed: Option<usize>,
    pub scale: f32,
}

pub fn paint(canvas: &mut PixmapMut, scene: &Scene) {
    let scale = scene.scale;
    let whole = Rect::new(
        0.0,
        0.0,
        canvas.width() as f32 / scale,
        canvas.height() as f32 / scale,
    );
    fill_rounded(canvas, whole, CORNER, &tint(24, 24, 28, 0.94), scale);
    stroke_rounded(
        canvas,
        whole.grown(-0.5),
        CORNER,
        1.0,
        &tint(255, 255, 255, 0.16),
        scale,
    );

    for (row, label) in scene.labels.iter().enumerate() {
        let top = MARGIN + row as f32 * ROW;
        if scene.pointed == Some(row) {
            let bar = Rect::new(MARGIN, top, whole.width - 2.0 * MARGIN, ROW);
            fill_rounded(canvas, bar, 6.0, &tint(255, 255, 255, 0.16), scale);
        }
        if let Some(label) = label {
            let (wide, tall) = (label.width() as f32 / scale, label.height() as f32 / scale);
            let place = Rect::new(MARGIN + INSET, top + (ROW - tall) / 2.0, wide, tall);
            picture(canvas, label, place, 0.0, Finish::Fine, scale);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::items::dock_items;
    use crate::paint::probes::*;
    use crate::windows::fixtures::*;
    use crate::windows::{AppId, WindowId};

    fn menu_for(app: &str, pinned: &[&str], windows: &Windows, pins_are_editable: bool) -> Vec<Entry> {
        let pinned: Vec<AppId> = pinned.iter().map(|app| AppId::new(*app)).collect();
        let items = dock_items(&pinned, windows, None);
        let item = items.iter().find(|item| item.app.as_str() == app).unwrap();
        entries(item, windows, pins_are_editable)
    }

    fn labels(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.label.as_str()).collect()
    }

    #[test]
    fn an_app_that_is_not_running_can_be_opened_or_unpinned() {
        let menu = menu_for("editor", &["editor"], &Windows::default(), true);

        assert_eq!(labels(&menu), ["Open", "Unpin from Dock"]);
        assert_eq!(menu[0].effects, [Effect::Launch(AppId::new("editor"))]);
        assert_eq!(menu[1].effects, [Effect::Unpin(AppId::new("editor"))]);
    }

    #[test]
    fn a_running_app_lists_its_windows_by_title_oldest_first() {
        let windows = used_in_order(&[focused(2, "editor"), window(1, "editor")]);

        let menu = menu_for("editor", &["editor"], &windows, true);

        assert_eq!(
            labels(&menu),
            ["editor 1", "editor 2", "New Window", "Unpin from Dock", "Quit"]
        );
        assert_eq!(menu[1].effects, [Effect::Activate(WindowId(2))]);
    }

    #[test]
    fn quit_closes_every_window_of_the_app() {
        let windows = used_in_order(&[focused(2, "editor"), window(1, "editor")]);

        let menu = menu_for("editor", &[], &windows, true);

        assert_eq!(
            menu.last().unwrap().effects,
            [Effect::Close(WindowId(1)), Effect::Close(WindowId(2))]
        );
    }

    #[test]
    fn a_running_app_that_is_not_pinned_offers_to_stay() {
        let windows = used_in_order(&[focused(1, "terminal")]);

        let menu = menu_for("terminal", &[], &windows, true);

        assert_eq!(labels(&menu), ["terminal 1", "New Window", "Pin to Dock", "Quit"]);
        assert_eq!(menu[2].effects, [Effect::Pin(AppId::new("terminal"))]);
    }

    #[test]
    fn pinning_is_not_offered_when_the_configuration_file_decides_the_pins() {
        let windows = used_in_order(&[focused(1, "terminal")]);

        let menu = menu_for("terminal", &[], &windows, false);

        assert_eq!(labels(&menu), ["terminal 1", "New Window", "Quit"]);
    }

    #[test]
    fn the_menu_is_as_wide_as_its_longest_label_within_limits() {
        assert_eq!(size(3, 20.0), (NARROWEST, 3.0 * ROW + 2.0 * MARGIN));
        assert_eq!(size(1, 200.0).0, 236.0);
        assert_eq!(size(1, 2000.0).0, WIDEST);
    }

    #[test]
    fn pointing_into_a_row_picks_it_and_the_margins_pick_nothing() {
        assert_eq!(row_at(3, MARGIN + 1.0), Some(0));
        assert_eq!(row_at(3, MARGIN + ROW * 2.5), Some(2));
        assert_eq!(row_at(3, MARGIN - 1.0), None);
        assert_eq!(row_at(3, MARGIN + ROW * 3.0 + 1.0), None);
    }

    #[test]
    fn each_label_is_written_on_its_row_and_the_pointed_row_is_lit() {
        let text = solid(12, 0, 255, 0);
        let labels = [Some(&text), Some(&text)];
        let (wide, tall) = size(2, 12.0);
        let mut pixmap = Pixmap::new(wide as u32, tall as u32).unwrap();

        paint(
            &mut pixmap.as_mut(),
            &Scene {
                labels: &labels,
                pointed: Some(1),
                scale: 1.0,
            },
        );

        let in_row = |row: f32| MARGIN + ROW * row + ROW / 2.0;
        assert_eq!(rgba(&pixmap, MARGIN + INSET + 6.0, in_row(0.0)), [0, 255, 0, 255]);
        assert_eq!(rgba(&pixmap, MARGIN + INSET + 6.0, in_row(1.0)), [0, 255, 0, 255]);
        assert!(rgba(&pixmap, wide - 20.0, in_row(1.0))[0] > rgba(&pixmap, wide - 20.0, in_row(0.0))[0] + 20);
        assert_eq!(alpha(&pixmap, 0.0, 0.0), 0);
    }
}
