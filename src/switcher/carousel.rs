use crate::geometry::Rect;

const RECESSION: f32 = 0.42;
const STEP: f32 = 0.47;
const CROWDING: f32 = 0.6;
const FARTHEST: f32 = 4.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cover {
    pub index: usize,
    pub frame: Rect,
    pub light: f32,
}

pub fn front_frame(stage: (f32, f32)) -> Rect {
    let (width, height) = stage;
    let tall = height * 0.56;
    let wide = (width * 0.4).min(tall * 1.6);
    Rect::new((width - wide) / 2.0, height * 0.14, wide, tall)
}

pub fn arrange(count: usize, focus: f32, stage: (f32, f32)) -> Vec<Cover> {
    let front = front_frame(stage);
    let mut covers: Vec<(f32, Cover)> = (0..count)
        .map(|index| (index as f32 - focus, index))
        .filter(|(offset, _)| offset.abs() <= FARTHEST)
        .map(|(offset, index)| {
            let depth = offset.abs();
            let scale = 1.0 / (1.0 + RECESSION * depth);
            let drift = front.width * STEP * (1.0 - CROWDING.powf(depth)) / (1.0 - CROWDING);
            let (wide, tall) = (front.width * scale, front.height * scale);
            let frame = Rect::new(
                front.center_x() + offset.signum() * drift - wide / 2.0,
                front.bottom() - tall,
                wide,
                tall,
            );
            (depth, Cover { index, frame, light: (1.0 - 0.3 * depth).max(0.3) })
        })
        .collect();
    covers.sort_by(|(a, _), (b, _)| b.total_cmp(a));
    covers.into_iter().map(|(_, cover)| cover).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAGE: (f32, f32) = (1600.0, 600.0);

    fn cover(covers: &[Cover], index: usize) -> Cover {
        *covers.iter().find(|cover| cover.index == index).unwrap()
    }

    #[test]
    fn the_selected_window_takes_center_stage_at_full_size_and_brightness() {
        let covers = arrange(5, 2.0, STAGE);

        let selected = cover(&covers, 2);

        assert_eq!(selected.frame, front_frame(STAGE));
        assert!((selected.frame.center_x() - 800.0).abs() < 0.01);
        assert_eq!(selected.light, 1.0);
    }

    #[test]
    fn it_is_painted_last_so_it_overlaps_its_neighbours() {
        let covers = arrange(5, 2.0, STAGE);

        assert_eq!(covers.last().unwrap().index, 2);
        assert!(cover(&covers, 3).frame.x < cover(&covers, 2).frame.right());
    }

    #[test]
    fn windows_recede_smaller_and_dimmer_to_both_sides() {
        let covers = arrange(5, 2.0, STAGE);
        let (left, near, far) = (cover(&covers, 1), cover(&covers, 3), cover(&covers, 4));

        assert!(near.frame.width < front_frame(STAGE).width && far.frame.width < near.frame.width);
        assert!(near.light < 1.0 && far.light < near.light);
        assert!(far.frame.center_x() > near.frame.center_x());
        assert!((left.frame.center_x() + near.frame.center_x() - 1600.0).abs() < 0.01);
        assert_eq!(left.frame.width, near.frame.width);
    }

    #[test]
    fn every_window_stands_on_the_same_floor() {
        let covers = arrange(5, 1.0, STAGE);

        assert!(covers.iter().all(|cover| (cover.frame.bottom() - front_frame(STAGE).bottom()).abs() < 0.01));
    }

    #[test]
    fn halfway_through_a_move_the_two_windows_share_the_center() {
        let covers = arrange(5, 1.5, STAGE);
        let (leaving, arriving) = (cover(&covers, 1), cover(&covers, 2));

        assert_eq!(leaving.frame.width, arriving.frame.width);
        assert!((leaving.frame.center_x() + arriving.frame.center_x() - 1600.0).abs() < 0.01);
        assert!(leaving.frame.width < front_frame(STAGE).width);
    }

    #[test]
    fn windows_too_far_from_the_selection_are_left_off_stage() {
        let covers = arrange(30, 10.0, STAGE);

        let shown: Vec<usize> = covers.iter().map(|cover| cover.index).collect();
        assert_eq!(shown.len(), 9);
        assert!(shown.contains(&6) && shown.contains(&14) && !shown.contains(&5) && !shown.contains(&15));
    }

    #[test]
    fn the_whole_row_fits_on_the_stage() {
        let covers = arrange(30, 10.0, STAGE);

        assert!(covers.iter().all(|cover| cover.frame.x >= 0.0 && cover.frame.right() <= 1600.0));
    }
}
