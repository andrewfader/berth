use serde::Deserialize;

use crate::windows::{WindowId, Windows};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Listed {
    #[default]
    All,
    ThisWorkspace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Next,
    Previous,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    candidates: Vec<WindowId>,
    selected: usize,
}

impl Session {
    pub fn begin(windows: &Windows, direction: Direction, listed: Listed) -> Option<Self> {
        let offered: Vec<_> = windows
            .most_recent_first()
            .iter()
            .filter(|window| listed == Listed::All || window.here)
            .collect();
        let candidates: Vec<WindowId> = offered.iter().map(|window| window.id).collect();
        let count = candidates.len();
        let focused_is_first = offered.first()?.active;
        let selected = match direction {
            Direction::Next if focused_is_first => 1 % count,
            Direction::Next => 0,
            Direction::Previous => count - 1,
        };
        Some(Self { candidates, selected })
    }

    pub fn step(&mut self, direction: Direction) {
        let count = self.candidates.len();
        self.selected = match direction {
            Direction::Next => (self.selected + 1) % count,
            Direction::Previous => (self.selected + count - 1) % count,
        };
    }

    pub fn forget(self, id: WindowId) -> Option<Self> {
        let Some(gone) = self.candidates.iter().position(|candidate| *candidate == id) else {
            return Some(self);
        };
        let mut candidates = self.candidates;
        candidates.remove(gone);
        let selected = if gone < self.selected {
            self.selected - 1
        } else {
            self.selected
        };
        (!candidates.is_empty()).then(|| Self {
            selected: selected % candidates.len(),
            candidates,
        })
    }

    pub fn selected(&self) -> WindowId {
        self.candidates[self.selected]
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn candidates(&self) -> &[WindowId] {
        &self.candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows::fixtures::*;

    fn three_windows() -> Windows {
        used_in_order(&[focused(1, "editor"), window(2, "browser"), window(3, "terminal")])
    }

    #[test]
    fn it_offers_windows_from_most_to_least_recently_used() {
        let session = Session::begin(&three_windows(), Direction::Next, Listed::All).unwrap();

        assert_eq!(session.candidates(), [WindowId(1), WindowId(2), WindowId(3)]);
    }

    #[test]
    fn it_opens_on_the_window_used_just_before_the_focused_one() {
        let session = Session::begin(&three_windows(), Direction::Next, Listed::All).unwrap();

        assert_eq!(session.selected(), WindowId(2));
    }

    #[test]
    fn opened_backwards_it_starts_on_the_least_recently_used_window() {
        let session = Session::begin(&three_windows(), Direction::Previous, Listed::All).unwrap();

        assert_eq!(session.selected(), WindowId(3));
    }

    #[test]
    fn when_no_window_has_focus_it_opens_on_the_most_recent_one() {
        let windows = used_in_order(&[window(1, "editor"), window(2, "browser")]);

        let session = Session::begin(&windows, Direction::Next, Listed::All).unwrap();

        assert_eq!(session.selected(), WindowId(1));
    }

    #[test]
    fn limited_to_this_workspace_it_skips_windows_that_are_elsewhere() {
        let windows = used_in_order(&[
            focused(1, "editor"),
            elsewhere(window(2, "browser")),
            window(3, "terminal"),
        ]);

        let session = Session::begin(&windows, Direction::Next, Listed::ThisWorkspace).unwrap();

        assert_eq!(session.candidates(), [WindowId(1), WindowId(3)]);
        assert_eq!(session.selected(), WindowId(3));
    }

    #[test]
    fn limited_to_this_workspace_there_may_be_nothing_to_switch_to() {
        let windows = used_in_order(&[elsewhere(window(1, "editor"))]);

        assert_eq!(
            Session::begin(&windows, Direction::Next, Listed::ThisWorkspace),
            None
        );
    }

    #[test]
    fn a_lone_window_is_its_own_alternative() {
        let windows = used_in_order(&[focused(1, "editor")]);

        let session = Session::begin(&windows, Direction::Next, Listed::All).unwrap();

        assert_eq!(session.selected(), WindowId(1));
    }

    #[test]
    fn there_is_nothing_to_switch_between_without_windows() {
        assert_eq!(
            Session::begin(&Windows::default(), Direction::Next, Listed::All),
            None
        );
    }

    #[test]
    fn stepping_wraps_around_at_both_ends() {
        let mut session = Session::begin(&three_windows(), Direction::Next, Listed::All).unwrap();

        session.step(Direction::Next);
        assert_eq!(session.selected(), WindowId(3));
        session.step(Direction::Next);
        assert_eq!(session.selected(), WindowId(1));
        session.step(Direction::Previous);
        assert_eq!(session.selected(), WindowId(3));
    }

    #[test]
    fn the_selection_stays_on_the_same_window_when_an_earlier_one_closes() {
        let mut session = Session::begin(&three_windows(), Direction::Next, Listed::All).unwrap();
        session.step(Direction::Next);

        let session = session.forget(WindowId(1)).unwrap();

        assert_eq!(session.selected(), WindowId(3));
    }

    #[test]
    fn the_selection_moves_on_when_the_selected_window_closes() {
        let session = Session::begin(&three_windows(), Direction::Previous, Listed::All).unwrap();

        let session = session.forget(WindowId(3)).unwrap();

        assert_eq!(session.selected(), WindowId(1));
    }

    #[test]
    fn it_ends_when_the_last_candidate_closes() {
        let windows = used_in_order(&[focused(1, "editor")]);
        let session = Session::begin(&windows, Direction::Next, Listed::All).unwrap();

        assert_eq!(session.forget(WindowId(1)), None);
    }
}
