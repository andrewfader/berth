use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct Animated {
    from: f32,
    to: f32,
    started: Duration,
    lasts: Duration,
}

impl Animated {
    pub fn resting_at(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            started: Duration::ZERO,
            lasts: Duration::ZERO,
        }
    }

    pub fn head_for(&mut self, target: f32, lasts: Duration, now: Duration) {
        if target == self.to {
            return;
        }
        self.from = self.value(now);
        self.to = target;
        self.started = now;
        self.lasts = lasts;
    }

    pub fn jump_to(&mut self, value: f32) {
        *self = Self::resting_at(value);
    }

    pub fn value(&self, now: Duration) -> f32 {
        if self.is_settled(now) {
            return self.to;
        }
        let progress = (now - self.started).as_secs_f32() / self.lasts.as_secs_f32();
        self.from + (self.to - self.from) * ease_out(progress)
    }

    pub fn target(&self) -> f32 {
        self.to
    }

    pub fn is_settled(&self, now: Duration) -> bool {
        now.saturating_sub(self.started) >= self.lasts
    }
}

fn ease_out(progress: f32) -> f32 {
    1.0 - (1.0 - progress).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn a_value_at_rest_does_not_move() {
        let value = Animated::resting_at(0.4);

        assert_eq!(value.value(500 * MS), 0.4);
        assert!(value.is_settled(Duration::ZERO));
    }

    #[test]
    fn it_arrives_exactly_at_the_target_when_the_time_is_up() {
        let mut value = Animated::resting_at(0.0);

        value.head_for(1.0, 200 * MS, 1000 * MS);

        assert_eq!(value.value(1200 * MS), 1.0);
        assert!(value.is_settled(1200 * MS));
    }

    #[test]
    fn it_covers_most_of_the_distance_early_and_eases_into_the_target() {
        let mut value = Animated::resting_at(0.0);

        value.head_for(1.0, 200 * MS, Duration::ZERO);

        let halfway = value.value(100 * MS);
        assert!(halfway > 0.8 && halfway < 1.0, "{halfway}");
        assert!(!value.is_settled(100 * MS));
    }

    #[test]
    fn changing_course_midway_continues_from_where_it_is() {
        let mut value = Animated::resting_at(0.0);
        value.head_for(1.0, 200 * MS, Duration::ZERO);
        let reached = value.value(100 * MS);

        value.head_for(0.0, 200 * MS, 100 * MS);

        assert_eq!(value.value(100 * MS), reached);
        assert_eq!(value.value(300 * MS), 0.0);
    }

    #[test]
    fn heading_for_the_current_target_again_does_not_restart_the_motion() {
        let mut value = Animated::resting_at(0.0);
        value.head_for(1.0, 200 * MS, Duration::ZERO);

        value.head_for(1.0, 200 * MS, 150 * MS);

        assert_eq!(value.value(200 * MS), 1.0);
    }
}
