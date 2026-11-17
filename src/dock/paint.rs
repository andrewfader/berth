use tiny_skia::{Pixmap, PixmapMut};

use super::layout::Layout;
use crate::geometry::Rect;
use crate::paint::{fill_rounded, picture, stroke_rounded, tint};

pub const CORNER: f32 = 14.0;

pub struct Tile<'a> {
    pub icon: &'a Pixmap,
    pub running: bool,
    pub focused: bool,
}

pub struct Scene<'a> {
    pub layout: &'a Layout,
    pub tiles: &'a [Tile<'a>],
    pub label: Option<(usize, &'a Pixmap)>,
    pub opacity: f32,
    pub scale: f32,
}

pub fn paint(canvas: &mut PixmapMut, scene: &Scene) {
    let (layout, scale) = (scene.layout, scene.scale);

    fill_rounded(
        canvas,
        layout.panel,
        CORNER,
        &tint(24, 24, 28, scene.opacity),
        scale,
    );
    stroke_rounded(
        canvas,
        layout.panel.grown(-0.5),
        CORNER,
        1.0,
        &tint(255, 255, 255, 0.16),
        scale,
    );

    for (tile, place) in scene.tiles.iter().zip(&layout.icons) {
        picture(canvas, tile.icon, *place, 0.0, 1.0, scale);
        if tile.running {
            let wide = if tile.focused { 14.0 } else { 4.0 };
            let mark = Rect::new(
                place.center_x() - wide / 2.0,
                layout.panel.bottom() - 6.0,
                wide,
                4.0,
            );
            fill_rounded(
                canvas,
                mark,
                2.0,
                &tint(255, 255, 255, if tile.focused { 0.95 } else { 0.7 }),
                scale,
            );
        }
    }

    if let Some((index, text)) = scene.label
        && let Some(place) = layout.icons.get(index)
    {
        let (wide, tall) = (text.width() as f32 / scale, text.height() as f32 / scale);
        let surface_width = canvas.width() as f32 / scale;
        let bubble = Rect::new(0.0, (place.y - tall - 10.0).max(0.0), wide + 20.0, tall + 4.0);
        let bubble = bubble.moved(
            (place.center_x() - bubble.width / 2.0).clamp(0.0, surface_width - bubble.width),
            0.0,
        );
        fill_rounded(canvas, bubble, bubble.height / 2.0, &tint(24, 24, 28, 0.9), scale);
        picture(
            canvas,
            text,
            Rect::new(bubble.x + 10.0, bubble.y + 2.0, wide, tall),
            0.0,
            1.0,
            scale,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::layout::{Metrics, Pose};
    use crate::paint::probes::*;

    struct Painted {
        pixmap: Pixmap,
        layout: Layout,
    }

    fn metrics() -> Metrics {
        Metrics::for_icon(48.0, 1.5)
    }

    fn painted(
        pose: Pose,
        tiles: &[(bool, bool)],
        label: Option<usize>,
        opacity: f32,
        scale: f32,
    ) -> Painted {
        let (width, height) = metrics().surface(tiles.len());
        let layout = Layout::arrange(&metrics(), tiles.len(), pose);
        let icon = solid(96, 255, 0, 0);
        let text = solid(16, 0, 255, 0);
        let tiles: Vec<Tile> = tiles
            .iter()
            .map(|(running, focused)| Tile {
                icon: &icon,
                running: *running,
                focused: *focused,
            })
            .collect();
        let mut pixmap = Pixmap::new((width * scale) as u32, (height * scale) as u32).unwrap();
        paint(
            &mut pixmap.as_mut(),
            &Scene {
                layout: &layout,
                tiles: &tiles,
                label: label.map(|index| (index, &text)),
                opacity,
                scale,
            },
        );
        Painted { pixmap, layout }
    }

    fn resting(tiles: &[(bool, bool)], opacity: f32) -> Painted {
        painted(Pose::RESTING, tiles, None, opacity, 1.0)
    }

    const IDLE: (bool, bool) = (false, false);
    const RUNNING: (bool, bool) = (true, false);
    const FOCUSED: (bool, bool) = (true, true);

    #[test]
    fn the_panel_lets_the_desktop_show_through_by_the_configured_amount() {
        let see_through = resting(&[IDLE, IDLE], 0.4);
        let solid_panel = resting(&[IDLE, IDLE], 1.0);
        let between_icons = (
            see_through.layout.icons[0].right() + 3.0,
            see_through.layout.panel.center_y(),
        );

        let faint = alpha(&see_through.pixmap, between_icons.0, between_icons.1);
        assert!((100..=104).contains(&faint), "{faint}");
        assert_eq!(alpha(&solid_panel.pixmap, between_icons.0, between_icons.1), 255);
    }

    #[test]
    fn nothing_is_painted_outside_the_panel_and_its_icons() {
        let dock = resting(&[IDLE, IDLE], 0.5);
        let panel = dock.layout.panel;

        assert_eq!(alpha(&dock.pixmap, panel.x - 2.0, panel.center_y()), 0);
        assert_eq!(alpha(&dock.pixmap, panel.center_x(), panel.y - 2.0), 0);
        assert_eq!(alpha(&dock.pixmap, panel.center_x(), panel.bottom() + 2.0), 0);
        assert_eq!(alpha(&dock.pixmap, panel.x + 1.0, panel.y + 1.0), 0);
    }

    #[test]
    fn every_app_icon_is_drawn_in_its_place() {
        let dock = resting(&[IDLE, IDLE, IDLE], 0.5);

        for icon in &dock.layout.icons {
            assert_eq!(
                rgba(&dock.pixmap, icon.center_x(), icon.center_y()),
                [255, 0, 0, 255]
            );
        }
    }

    #[test]
    fn a_magnified_icon_is_drawn_larger_rising_above_the_panel() {
        let rest = resting(&[IDLE, IDLE, IDLE], 0.5);
        let over_the_middle = Pose {
            pointer: Some(rest.layout.icons[1].center_x()),
            zoom: 1.0,
            shown: 1.0,
        };

        let dock = painted(over_the_middle, &[IDLE, IDLE, IDLE], None, 0.5, 1.0);

        let above_the_panel = (dock.layout.icons[1].center_x(), dock.layout.panel.y - 6.0);
        assert_eq!(
            rgba(&dock.pixmap, above_the_panel.0, above_the_panel.1),
            [255, 0, 0, 255]
        );
        assert_eq!(alpha(&rest.pixmap, above_the_panel.0, above_the_panel.1), 0);
    }

    #[test]
    fn only_running_apps_get_a_mark_and_the_focused_one_a_wider_mark() {
        let dock = resting(&[IDLE, RUNNING, FOCUSED], 0.5);
        let under = |index: usize, aside: f32| {
            rgba(
                &dock.pixmap,
                dock.layout.icons[index].center_x() + aside,
                dock.layout.panel.bottom() - 4.0,
            )
        };

        assert_ne!(under(0, 0.0)[0], under(1, 0.0)[0]);
        assert!(under(1, 0.0)[0] > 150 && under(2, 0.0)[0] > 200);
        assert_eq!(under(1, 5.0), under(0, 5.0));
        assert!(under(2, 5.0)[0] > 200);
    }

    #[test]
    fn the_name_of_the_app_under_the_pointer_floats_above_its_icon() {
        let rest = resting(&[IDLE, IDLE, IDLE], 0.5);
        let pointer = rest.layout.icons[2].center_x();

        let dock = painted(
            Pose {
                pointer: Some(pointer),
                zoom: 1.0,
                shown: 1.0,
            },
            &[IDLE, IDLE, IDLE],
            Some(2),
            0.5,
            1.0,
        );

        let icon = dock.layout.icons[2];
        assert_eq!(
            rgba(&dock.pixmap, icon.center_x(), icon.y - 18.0),
            [0, 255, 0, 255]
        );
        assert_eq!(alpha(&rest.pixmap, icon.center_x(), icon.y - 18.0), 0);
    }

    #[test]
    fn a_hidden_dock_paints_nothing_at_all() {
        let dock = painted(
            Pose {
                shown: 0.0,
                ..Pose::RESTING
            },
            &[RUNNING, FOCUSED],
            None,
            0.5,
            1.0,
        );

        assert!(dock.pixmap.pixels().iter().all(|pixel| pixel.alpha() == 0));
    }

    #[test]
    fn on_a_dense_display_everything_is_drawn_at_double_resolution() {
        let dock = painted(Pose::RESTING, &[IDLE], None, 0.5, 2.0);
        let icon = dock.layout.icons[0];

        assert_eq!(
            rgba(&dock.pixmap, icon.right() * 2.0 - 2.0, icon.bottom() * 2.0 - 2.0),
            [255, 0, 0, 255]
        );
        assert_eq!(
            rgba(&dock.pixmap, icon.x * 2.0 + 2.0, icon.y * 2.0 + 2.0),
            [255, 0, 0, 255]
        );
    }
}
