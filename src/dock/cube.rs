use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use super::bounce::PATIENCE;
use crate::geometry::Rect;

pub const QUARTER_TURN: Duration = Duration::from_millis(420);
const EDGE: f32 = 0.72;
const DEEPEST_SHADE: f32 = 0.6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub place: Rect,
    pub shade: f32,
}

pub fn faces(since_launch: Duration, slot: Rect) -> Vec<Face> {
    if since_launch >= PATIENCE {
        return vec![Face {
            place: slot,
            shade: 0.0,
        }];
    }
    let turned = (since_launch.as_secs_f32() / QUARTER_TURN.as_secs_f32()).fract() * FRAC_PI_2;
    let edge = slot.width * EDGE;
    let (leaving, arriving) = (edge * turned.cos(), edge * turned.sin());
    let left = slot.center_x() - (leaving + arriving) / 2.0;
    let top = slot.center_y() - edge / 2.0;
    [
        (left, leaving, turned.cos()),
        (left + leaving, arriving, turned.sin()),
    ]
    .into_iter()
    .filter(|(_, wide, _)| *wide >= 0.5)
    .map(|(x, wide, facing)| Face {
        place: Rect::new(x, top, wide, edge),
        shade: (1.0 - facing) * DEEPEST_SHADE,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOT: Rect = Rect {
        x: 100.0,
        y: 50.0,
        width: 48.0,
        height: 48.0,
    };

    #[test]
    fn at_the_start_of_a_turn_one_face_looks_straight_at_you() {
        let faces = faces(Duration::ZERO, SLOT);

        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].shade, 0.0);
        assert!((faces[0].place.center_x() - SLOT.center_x()).abs() < 0.01);
        assert!(faces[0].place.width < SLOT.width && faces[0].place.width == faces[0].place.height);
    }

    #[test]
    fn halfway_through_a_turn_two_faces_share_the_view_edge_to_edge() {
        let faces = faces(QUARTER_TURN / 2, SLOT);

        assert_eq!(faces.len(), 2);
        assert!((faces[0].place.right() - faces[1].place.x).abs() < 0.01);
        assert!((faces[0].place.width - faces[1].place.width).abs() < 0.01);
        assert!((faces[0].shade - faces[1].shade).abs() < 0.01 && faces[0].shade > 0.1);
        let middle = (faces[0].place.x + faces[1].place.right()) / 2.0;
        assert!((middle - SLOT.center_x()).abs() < 0.01);
    }

    #[test]
    fn the_face_turning_away_narrows_and_darkens_as_the_next_one_comes_round() {
        let early = faces(QUARTER_TURN / 4, SLOT);
        let late = faces(QUARTER_TURN * 3 / 4, SLOT);

        assert!(late[0].place.width < early[0].place.width && late[0].shade > early[0].shade);
        assert!(late[1].place.width > early[1].place.width && late[1].shade < early[1].shade);
    }

    #[test]
    fn the_cube_keeps_turning_quarter_after_quarter() {
        let first = faces(QUARTER_TURN / 3, SLOT);
        let fifth = faces(QUARTER_TURN * 4 + QUARTER_TURN / 3, SLOT);

        assert!((first[0].place.width - fifth[0].place.width).abs() < 0.05);
    }

    #[test]
    fn even_at_its_widest_the_cube_barely_leaves_its_slot() {
        let widest = faces(QUARTER_TURN / 2, SLOT);

        assert!(widest[1].place.right() - widest[0].place.x < SLOT.width * 1.05);
    }

    #[test]
    fn an_app_that_never_shows_up_gets_its_plain_icon_back() {
        assert_eq!(
            faces(PATIENCE, SLOT),
            [Face {
                place: SLOT,
                shade: 0.0
            }]
        );
    }
}
