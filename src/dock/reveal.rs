use std::time::Duration;

use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Autohide {
    Never,
    Always,
    #[default]
    Intelligent,
}

pub const LINGER: Duration = Duration::from_millis(350);

#[derive(Clone, Copy, Debug)]
pub struct Reveal {
    autohide: Autohide,
    hovered: bool,
    obstructed: bool,
    unwanted_since: Option<Duration>,
}

impl Reveal {
    pub fn new(autohide: Autohide) -> Self {
        Self {
            autohide,
            hovered: false,
            obstructed: false,
            unwanted_since: None,
        }
    }

    pub fn pointer_entered(&mut self, now: Duration) {
        self.hovered = true;
        self.reconsider(now);
    }

    pub fn pointer_left(&mut self, now: Duration) {
        self.hovered = false;
        self.reconsider(now);
    }

    pub fn obstruction_changed(&mut self, obstructed: bool, now: Duration) {
        self.obstructed = obstructed;
        self.reconsider(now);
    }

    pub fn is_shown(&self, now: Duration) -> bool {
        self.hides_at().is_none_or(|deadline| now < deadline)
    }

    pub fn hides_at(&self) -> Option<Duration> {
        self.unwanted_since.map(|since| since + LINGER)
    }

    fn reconsider(&mut self, now: Duration) {
        if self.is_wanted() {
            self.unwanted_since = None;
        } else if self.unwanted_since.is_none() {
            self.unwanted_since = Some(now);
        }
    }

    fn is_wanted(&self) -> bool {
        match self.autohide {
            Autohide::Never => true,
            Autohide::Always => self.hovered,
            Autohide::Intelligent => self.hovered || !self.obstructed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: Duration = Duration::from_secs(10);
    const SOON: Duration = Duration::from_millis(10_100);
    const LATER: Duration = Duration::from_secs(11);

    #[test]
    fn a_dock_that_never_hides_stays_put_under_a_window() {
        let mut reveal = Reveal::new(Autohide::Never);

        reveal.obstruction_changed(true, START);

        assert!(reveal.is_shown(LATER));
        assert_eq!(reveal.hides_at(), None);
    }

    #[test]
    fn an_intelligent_dock_stays_while_no_window_needs_its_space() {
        let reveal = Reveal::new(Autohide::Intelligent);

        assert!(reveal.is_shown(LATER));
    }

    #[test]
    fn an_intelligent_dock_makes_way_shortly_after_a_window_covers_it() {
        let mut reveal = Reveal::new(Autohide::Intelligent);

        reveal.obstruction_changed(true, START);

        assert!(reveal.is_shown(SOON));
        assert!(!reveal.is_shown(LATER));
        assert_eq!(reveal.hides_at(), Some(START + LINGER));
    }

    #[test]
    fn it_comes_back_the_moment_the_window_moves_away() {
        let mut reveal = Reveal::new(Autohide::Intelligent);
        reveal.obstruction_changed(true, START);

        reveal.obstruction_changed(false, LATER);

        assert!(reveal.is_shown(LATER));
    }

    #[test]
    fn the_pointer_brings_a_covered_dock_back_and_keeps_it_while_hovering() {
        let mut reveal = Reveal::new(Autohide::Intelligent);
        reveal.obstruction_changed(true, START);

        reveal.pointer_entered(LATER);

        assert!(reveal.is_shown(LATER + Duration::from_secs(60)));
    }

    #[test]
    fn it_lingers_briefly_after_the_pointer_leaves_so_a_slip_does_not_hide_it() {
        let mut reveal = Reveal::new(Autohide::Intelligent);
        reveal.obstruction_changed(true, Duration::ZERO);
        reveal.pointer_entered(START);

        reveal.pointer_left(START);
        assert!(reveal.is_shown(SOON));
        reveal.pointer_entered(SOON);

        assert!(reveal.is_shown(LATER));
    }

    #[test]
    fn a_dock_that_always_hides_only_shows_under_the_pointer() {
        let mut reveal = Reveal::new(Autohide::Always);
        reveal.pointer_left(START);
        assert!(!reveal.is_shown(LATER));

        reveal.pointer_entered(LATER);

        assert!(reveal.is_shown(LATER));
    }
}
