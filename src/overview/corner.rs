use serde::Deserialize;

use crate::geometry::Rect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HotCorner {
    #[default]
    TopLeft,
    TopRight,
    Off,
}

impl HotCorner {
    pub fn is_hot_on(self, monitor: Rect, others: &[Rect]) -> bool {
        let (x, y, outward) = match self {
            HotCorner::TopLeft => (monitor.x, monitor.y, -0.5),
            HotCorner::TopRight => (monitor.right(), monitor.y, 0.5),
            HotCorner::Off => return false,
        };
        let inward = -outward;
        let beyond = [
            (x + outward, y - 0.5),
            (x + outward, y + 0.5),
            (x + inward, y - 0.5),
        ];
        !others
            .iter()
            .filter(|other| **other != monitor)
            .any(|other| beyond.iter().any(|(bx, by)| other.contains(*bx, *by)))
    }

    pub fn is_reached(self, at: (f32, f32), screen: (u32, u32)) -> bool {
        let (x, y) = at;
        y < 1.0
            && match self {
                HotCorner::TopLeft => x < 1.0,
                HotCorner::TopRight => x >= screen.0 as f32 - 1.0,
                HotCorner::Off => false,
            }
    }
}

#[derive(Debug, Default)]
pub struct Poke {
    spent: bool,
}

impl Poke {
    pub fn arrived(&mut self) -> bool {
        !std::mem::replace(&mut self.spent, true)
    }

    pub fn left(&mut self) {
        self.spent = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };
    const RIGHT: Rect = Rect {
        x: 1920.0,
        y: 0.0,
        width: 1280.0,
        height: 720.0,
    };

    #[test]
    fn a_lone_monitor_has_a_hot_corner_wherever_it_is_set() {
        assert!(HotCorner::TopLeft.is_hot_on(LEFT, &[LEFT]));
        assert!(HotCorner::TopRight.is_hot_on(LEFT, &[LEFT]));
        assert!(!HotCorner::Off.is_hot_on(LEFT, &[LEFT]));
    }

    #[test]
    fn a_corner_that_leads_onto_the_next_monitor_is_not_hot() {
        let both = [LEFT, RIGHT];

        assert!(HotCorner::TopLeft.is_hot_on(LEFT, &both));
        assert!(!HotCorner::TopLeft.is_hot_on(RIGHT, &both));
        assert!(!HotCorner::TopRight.is_hot_on(LEFT, &both));
        assert!(HotCorner::TopRight.is_hot_on(RIGHT, &both));
    }

    #[test]
    fn a_monitor_stacked_above_takes_the_heat_from_the_corner_below_it() {
        let above = Rect::new(0.0, -1080.0, 1920.0, 1080.0);

        assert!(!HotCorner::TopLeft.is_hot_on(LEFT, &[LEFT, above]));
        assert!(HotCorner::TopLeft.is_hot_on(above, &[LEFT, above]));
    }

    #[test]
    fn a_taller_neighbour_that_only_touches_lower_down_still_blocks_the_corner() {
        let lower = Rect::new(-1280.0, 200.0, 1280.0, 720.0);
        let alongside = Rect::new(-1280.0, -100.0, 1280.0, 1440.0);

        assert!(HotCorner::TopLeft.is_hot_on(LEFT, &[LEFT, lower]));
        assert!(!HotCorner::TopLeft.is_hot_on(LEFT, &[LEFT, alongside]));
    }

    #[test]
    fn the_corner_is_reached_only_in_its_outermost_pixel() {
        let screen = (1920, 1080);

        assert!(HotCorner::TopLeft.is_reached((0.0, 0.0), screen));
        assert!(!HotCorner::TopLeft.is_reached((1.0, 0.0), screen));
        assert!(!HotCorner::TopLeft.is_reached((0.0, 1.5), screen));
        assert!(HotCorner::TopRight.is_reached((1919.5, 0.0), screen));
        assert!(!HotCorner::TopRight.is_reached((0.0, 0.0), screen));
        assert!(!HotCorner::Off.is_reached((0.0, 0.0), screen));
    }

    #[test]
    fn a_poke_fires_once_until_the_pointer_has_left_the_corner() {
        let mut poke = Poke::default();

        assert!(poke.arrived());
        assert!(!poke.arrived());
        poke.left();
        assert!(poke.arrived());
    }
}
