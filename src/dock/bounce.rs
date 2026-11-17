use std::f32::consts::PI;
use std::time::Duration;

pub const HOP: Duration = Duration::from_millis(560);
pub const PATIENCE: Duration = Duration::from_secs(8);

pub fn lift(since_launch: Duration, icon: f32) -> f32 {
    if since_launch >= PATIENCE {
        return 0.0;
    }
    let through_the_hop = since_launch.as_secs_f32() / HOP.as_secs_f32();
    (through_the_hop.fract() * PI).sin() * icon * 0.3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_starting_app_hops_up_and_lands_again_over_and_over() {
        assert_eq!(lift(Duration::ZERO, 48.0), 0.0);
        assert!((lift(HOP / 2, 48.0) - 14.4).abs() < 0.01);
        assert!(lift(HOP, 48.0) < 0.01);
        assert!((lift(HOP * 3 + HOP / 2, 48.0) - 14.4).abs() < 0.05);
    }

    #[test]
    fn a_bigger_icon_hops_higher() {
        assert!(lift(HOP / 2, 96.0) > lift(HOP / 2, 48.0));
    }

    #[test]
    fn an_app_that_never_shows_up_stops_hopping() {
        assert_eq!(lift(PATIENCE, 48.0), 0.0);
        assert_eq!(lift(PATIENCE + HOP / 2, 48.0), 0.0);
    }
}
