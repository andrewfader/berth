use std::f32::consts::FRAC_PI_2;

use serde::Deserialize;

use crate::geometry::Rect;

pub const SMALLEST_ICON: f32 = 16.0;
const LABEL_ABOVE: f32 = 30.0;
const LABEL_BESIDE: f32 = 200.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Edge {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

impl Edge {
    pub fn is_upright(self) -> bool {
        matches!(self, Edge::Left | Edge::Right)
    }

    pub fn surface(self, lying: (f32, f32)) -> (f32, f32) {
        if self.is_upright() {
            (lying.1, lying.0)
        } else {
            lying
        }
    }

    pub fn place(self, rect: Rect, lying: (f32, f32)) -> Rect {
        match self {
            Edge::Bottom => rect,
            Edge::Top => Rect::new(rect.x, lying.1 - rect.bottom(), rect.width, rect.height),
            Edge::Left => Rect::new(lying.1 - rect.bottom(), rect.x, rect.height, rect.width),
            Edge::Right => Rect::new(rect.y, rect.x, rect.height, rect.width),
        }
    }

    pub fn along(self, x: f32, y: f32) -> f32 {
        if self.is_upright() { y } else { x }
    }

    pub fn inward(self, distance: f32) -> (f32, f32) {
        match self {
            Edge::Bottom => (0.0, -distance),
            Edge::Top => (0.0, distance),
            Edge::Left => (distance, 0.0),
            Edge::Right => (-distance, 0.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub icon: f32,
    pub gap: f32,
    pub padding: f32,
    pub lift: f32,
    pub magnification: f32,
    pub reach: f32,
    pub label: f32,
}

impl Metrics {
    pub fn for_icon(icon: f32, magnification: f32) -> Self {
        Self {
            icon,
            gap: (icon / 8.0).round(),
            padding: (icon / 6.0).round(),
            lift: 6.0,
            magnification,
            reach: icon * 2.5,
            label: LABEL_ABOVE,
        }
    }

    pub fn fitted(icon: f32, magnification: f32, items: usize, room: f32) -> Self {
        let mut metrics = Self::for_icon(icon, magnification);
        while metrics.surface(items).0 > room && metrics.icon > SMALLEST_ICON {
            metrics = Self::for_icon((metrics.icon - 2.0).max(SMALLEST_ICON), magnification);
        }
        if metrics.surface(items).0 > room {
            metrics = Self::for_icon(SMALLEST_ICON, 1.0);
        }
        metrics
    }

    pub fn along(self, edge: Edge) -> Self {
        Self {
            label: if edge.is_upright() {
                LABEL_BESIDE
            } else {
                LABEL_ABOVE
            },
            ..self
        }
    }

    pub fn surface(&self, items: usize) -> (f32, f32) {
        let width = self.resting_width(items) + 2.0 * self.widest_spread();
        let height = self.label + self.icon * self.magnification + self.padding + self.lift;
        (width.ceil(), height.ceil())
    }

    pub fn travel(&self) -> f32 {
        self.panel_height() + self.lift
    }

    fn pitch(&self) -> f32 {
        self.icon + self.gap
    }

    fn panel_height(&self) -> f32 {
        self.icon + 2.0 * self.padding
    }

    fn resting_width(&self, items: usize) -> f32 {
        let items = items.max(1) as f32;
        items * self.icon + (items - 1.0) * self.gap + 2.0 * self.padding
    }

    fn widest_spread(&self) -> f32 {
        (self.magnification - 1.0) * self.icon * (2.0 * self.reach / self.pitch() + 1.0)
    }

    fn swell(&self, distance: f32) -> f32 {
        let nearness = (1.0 - distance.abs() / self.reach).max(0.0);
        (nearness * FRAC_PI_2).sin().powi(2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub pointer: Option<f32>,
    pub zoom: f32,
    pub shown: f32,
}

impl Pose {
    pub const RESTING: Pose = Pose {
        pointer: None,
        zoom: 0.0,
        shown: 1.0,
    };
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub icons: Vec<Rect>,
    pub panel: Rect,
}

impl Layout {
    pub fn arrange(metrics: &Metrics, items: usize, pose: Pose) -> Self {
        let (width, height) = metrics.surface(items);
        let first_left = (width - metrics.resting_width(items)) / 2.0 + metrics.padding;
        let sunk = (1.0 - pose.shown) * metrics.travel();
        let baseline = height - metrics.lift - metrics.padding + sunk;

        let sizes: Vec<f32> = (0..items)
            .map(|index| {
                let resting_center = first_left + index as f32 * metrics.pitch() + metrics.icon / 2.0;
                let swell = pose
                    .pointer
                    .map_or(0.0, |pointer| metrics.swell(pointer - resting_center));
                metrics.icon * (1.0 + (metrics.magnification - 1.0) * pose.zoom * swell)
            })
            .collect();

        let mut lefts = Vec::with_capacity(items);
        let mut cursor = 0.0;
        for size in &sizes {
            lefts.push(cursor);
            cursor += size + metrics.gap;
        }

        let anchor = pose.pointer.unwrap_or(first_left);
        let resting_offset = anchor - first_left;
        let slot = (resting_offset / metrics.pitch())
            .floor()
            .clamp(0.0, items.saturating_sub(1) as f32);
        let within = resting_offset - slot * metrics.pitch();
        let stretched = match sizes.get(slot as usize) {
            Some(size) if (0.0..metrics.icon).contains(&within) => within * size / metrics.icon,
            Some(size) if within >= metrics.icon => size + (within - metrics.icon),
            _ => within,
        };
        let shift = anchor - (lefts.get(slot as usize).copied().unwrap_or(0.0) + stretched);

        let icons: Vec<Rect> = lefts
            .iter()
            .zip(&sizes)
            .map(|(left, size)| Rect::new(left + shift, baseline - size, *size, *size))
            .collect();

        let left = icons.first().map_or(first_left, |icon| icon.x) - metrics.padding;
        let right = icons
            .last()
            .map_or(first_left + metrics.icon, |icon| icon.right())
            + metrics.padding;
        let panel = Rect::new(
            left,
            baseline - metrics.icon - metrics.padding,
            right - left,
            metrics.panel_height(),
        );

        Self { icons, panel }
    }

    pub fn on(&self, edge: Edge, lying: (f32, f32)) -> Self {
        Self {
            icons: self.icons.iter().map(|icon| edge.place(*icon, lying)).collect(),
            panel: edge.place(self.panel, lying),
        }
    }

    pub fn item_at(metrics: &Metrics, items: usize, x: f32) -> Option<usize> {
        let (width, _) = metrics.surface(items);
        let panel_left = (width - metrics.resting_width(items)) / 2.0;
        let offset = x - panel_left - metrics.padding + metrics.gap / 2.0;
        let index = (offset / metrics.pitch()).floor();
        let inside = x >= panel_left && x < width - panel_left;
        (inside && items > 0).then(|| index.clamp(0.0, (items - 1) as f32) as usize)
    }

    pub fn reach(&self) -> Rect {
        self.icons
            .iter()
            .fold(self.panel, |reach, icon| reach.union(icon))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> Metrics {
        Metrics::for_icon(48.0, 1.5)
    }

    fn hovering(x: f32) -> Pose {
        Pose {
            pointer: Some(x),
            zoom: 1.0,
            shown: 1.0,
        }
    }

    fn resting(items: usize) -> Layout {
        Layout::arrange(&metrics(), items, Pose::RESTING)
    }

    #[test]
    fn at_rest_the_icons_sit_in_a_centered_row_inside_the_panel() {
        let (width, height) = metrics().surface(5);

        let layout = resting(5);

        assert_eq!(layout.icons.len(), 5);
        assert!(
            layout
                .icons
                .iter()
                .all(|icon| icon.width == 48.0 && icon.height == 48.0)
        );
        assert!((layout.panel.center_x() - width / 2.0).abs() < 0.01);
        assert!(
            layout
                .icons
                .iter()
                .all(|icon| icon.y > layout.panel.y && icon.bottom() < layout.panel.bottom())
        );
        assert!(layout.panel.bottom() < height);
    }

    #[test]
    fn the_icon_under_the_pointer_grows_to_the_full_magnification() {
        let middle = resting(7).icons[3];

        let layout = Layout::arrange(&metrics(), 7, hovering(middle.center_x()));

        assert!((layout.icons[3].width - 72.0).abs() < 0.01);
    }

    #[test]
    fn neighbours_swell_less_the_farther_they_are_and_distant_icons_not_at_all() {
        let middle = resting(9).icons[4];

        let layout = Layout::arrange(&metrics(), 9, hovering(middle.center_x()));
        let sizes: Vec<f32> = layout.icons.iter().map(|icon| icon.width).collect();

        assert!(sizes[4] > sizes[5] && sizes[5] > sizes[6] && sizes[6] > sizes[7]);
        assert_eq!(sizes[8], 48.0);
        assert_eq!(sizes[0], 48.0);
    }

    #[test]
    fn magnified_icons_grow_upward_from_a_fixed_baseline_and_never_overlap() {
        let rest = resting(7);

        let layout = Layout::arrange(&metrics(), 7, hovering(rest.icons[2].center_x()));

        assert!(
            layout
                .icons
                .iter()
                .all(|icon| icon.bottom() == rest.icons[0].bottom())
        );
        assert!(layout.icons.windows(2).all(|pair| pair[0].right() < pair[1].x));
        assert!(layout.icons[2].y < layout.panel.y);
    }

    #[test]
    fn the_spot_under_the_pointer_stays_under_the_pointer_while_the_dock_swells() {
        let rest = resting(7);
        for (index, icon) in rest.icons.iter().enumerate() {
            for pointer in [icon.x + 1.0, icon.center_x(), icon.right() - 1.0] {
                let layout = Layout::arrange(&metrics(), 7, hovering(pointer));

                assert!(
                    layout.icons[index].contains(pointer, layout.icons[index].center_y()),
                    "icon {index}"
                );
                assert_eq!(Layout::item_at(&metrics(), 7, pointer), Some(index));
            }
        }
    }

    #[test]
    fn the_magnified_dock_always_fits_its_surface() {
        let (width, _) = metrics().surface(12);
        let rest = resting(12);

        for pointer in [
            rest.panel.x,
            rest.icons[0].center_x(),
            rest.icons[6].x,
            rest.icons[11].right(),
            rest.panel.right(),
        ] {
            let layout = Layout::arrange(&metrics(), 12, hovering(pointer));

            assert!(
                layout.reach().x >= 0.0 && layout.reach().right() <= width,
                "pointer at {pointer}"
            );
            assert!(layout.reach().y >= metrics().label - 0.01);
        }
    }

    #[test]
    fn zoom_eases_between_rest_and_full_magnification() {
        let middle = resting(5).icons[2];

        let halfway = Layout::arrange(
            &metrics(),
            5,
            Pose {
                pointer: Some(middle.center_x()),
                zoom: 0.5,
                shown: 1.0,
            },
        );

        assert!((halfway.icons[2].width - 60.0).abs() < 0.01);
    }

    #[test]
    fn a_hidden_dock_has_sunk_completely_below_its_surface() {
        let (_, height) = metrics().surface(5);

        let hidden = Layout::arrange(
            &metrics(),
            5,
            Pose {
                shown: 0.0,
                ..Pose::RESTING
            },
        );

        assert!(hidden.panel.y >= height);
    }

    #[test]
    fn pointing_between_two_icons_picks_the_nearer_one_and_outside_the_panel_picks_none() {
        let rest = resting(4);
        let seam = (rest.icons[1].right() + rest.icons[2].x) / 2.0;

        assert_eq!(Layout::item_at(&metrics(), 4, seam - 1.0), Some(1));
        assert_eq!(Layout::item_at(&metrics(), 4, seam + 1.0), Some(2));
        assert_eq!(Layout::item_at(&metrics(), 4, rest.panel.x - 1.0), None);
        assert_eq!(Layout::item_at(&metrics(), 4, rest.panel.right() + 1.0), None);
        assert_eq!(Layout::item_at(&metrics(), 0, rest.panel.center_x()), None);
    }

    #[test]
    fn a_dock_with_room_to_spare_keeps_the_icon_size_asked_for() {
        assert_eq!(
            Metrics::fitted(48.0, 1.5, 5, 1920.0),
            Metrics::for_icon(48.0, 1.5)
        );
    }

    #[test]
    fn a_crowded_dock_shrinks_its_icons_until_it_fits_the_screen() {
        let crowded = Metrics::fitted(48.0, 1.5, 30, 1280.0);

        assert!(crowded.icon < 48.0 && crowded.icon > SMALLEST_ICON);
        assert!(crowded.surface(30).0 <= 1280.0);
        assert!(Metrics::for_icon(crowded.icon + 2.0, 1.5).surface(30).0 > 1280.0);
    }

    #[test]
    fn icons_never_shrink_below_a_size_that_can_still_be_hit() {
        assert_eq!(Metrics::fitted(48.0, 1.5, 500, 800.0).icon, SMALLEST_ICON);
    }

    #[test]
    fn a_dock_still_too_long_at_the_smallest_size_gives_up_its_swell_to_gain_room() {
        let smallest_swelling = Metrics::for_icon(SMALLEST_ICON, 1.5).surface(40).0;

        let squeezed = Metrics::fitted(48.0, 1.5, 40, smallest_swelling - 1.0);

        assert_eq!((squeezed.icon, squeezed.magnification), (SMALLEST_ICON, 1.0));
        assert!(squeezed.surface(40).0 < smallest_swelling);
    }

    fn placed(edge: Edge) -> (Layout, (f32, f32)) {
        let lying = metrics().surface(3);
        (resting(3).on(edge, lying), edge.surface(lying))
    }

    #[test]
    fn at_the_top_the_panel_hugs_the_upper_edge_and_icons_keep_their_order() {
        let ((top, _), (bottom, (_, height))) = (placed(Edge::Top), placed(Edge::Bottom));

        assert_eq!(top.panel.y, height - bottom.panel.bottom());
        assert!(top.panel.y < 10.0);
        assert!(top.icons[0].x < top.icons[1].x && top.icons[1].x < top.icons[2].x);
        assert_eq!(top.icons[0].x, bottom.icons[0].x);
    }

    #[test]
    fn at_the_left_the_dock_stands_upright_against_the_left_edge() {
        let (left, (width, height)) = placed(Edge::Left);

        assert!(height > width);
        assert!(left.panel.x < 10.0 && left.panel.height > left.panel.width);
        assert!(left.icons[0].y < left.icons[1].y && left.icons[1].y < left.icons[2].y);
        assert!(
            left.icons
                .iter()
                .all(|icon| icon.x > left.panel.x && icon.right() < left.panel.right())
        );
    }

    #[test]
    fn at_the_right_the_dock_stands_upright_against_the_right_edge() {
        let (right, (width, _)) = placed(Edge::Right);

        assert!(width - right.panel.right() < 10.0 && right.panel.height > right.panel.width);
        assert!(right.icons[0].y < right.icons[1].y);
        assert!(
            right
                .icons
                .iter()
                .all(|icon| icon.width == 48.0 && icon.height == 48.0)
        );
    }

    #[test]
    fn an_upright_dock_follows_the_pointer_up_and_down() {
        assert_eq!(Edge::Left.along(3.0, 250.0), 250.0);
        assert_eq!(Edge::Bottom.along(3.0, 250.0), 3.0);
    }

    #[test]
    fn an_upright_dock_leaves_room_beside_it_for_the_name_of_an_app() {
        let beside = metrics().along(Edge::Right);

        assert!(beside.surface(3).1 > metrics().surface(3).1 + 100.0);
        assert_eq!(metrics().along(Edge::Top), metrics());
    }

    #[test]
    fn inward_always_points_away_from_the_screen_edge() {
        assert_eq!(Edge::Bottom.inward(5.0), (0.0, -5.0));
        assert_eq!(Edge::Top.inward(5.0), (0.0, 5.0));
        assert_eq!(Edge::Left.inward(5.0), (5.0, 0.0));
        assert_eq!(Edge::Right.inward(5.0), (-5.0, 0.0));
    }

    #[test]
    fn an_empty_dock_still_shows_a_panel() {
        let layout = resting(0);

        assert!(layout.icons.is_empty());
        assert!(layout.panel.width > 48.0);
    }
}
