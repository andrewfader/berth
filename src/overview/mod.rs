pub mod corner;
pub mod layout;
pub mod paint;

use crate::windows::{AppId, WindowId};

pub const MOST_APPS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Pick {
    Window(WindowId),
    App(AppId),
}

pub struct Overview {
    windows: Vec<(WindowId, String)>,
    apps: Vec<(AppId, String)>,
    query: String,
    selected: usize,
}

impl Overview {
    pub fn open(windows: Vec<(WindowId, String)>, apps: Vec<(AppId, String)>) -> Self {
        Self {
            windows,
            apps,
            query: String::new(),
            selected: 0,
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn typed(&mut self, text: &str) {
        self.query
            .extend(text.chars().filter(|letter| !letter.is_control()));
        self.selected = 0;
    }

    pub fn erased(&mut self) {
        self.query.pop();
        self.selected = 0;
    }

    pub fn windows(&self) -> Vec<WindowId> {
        let wanted = self.query.to_lowercase();
        self.windows
            .iter()
            .filter(|(_, text)| text.to_lowercase().contains(&wanted))
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn apps(&self) -> Vec<AppId> {
        let wanted = self.query.to_lowercase();
        if wanted.is_empty() {
            return Vec::new();
        }
        let named = |from_the_start: bool| {
            let wanted = wanted.clone();
            self.apps.iter().filter(move |(_, name)| {
                let name = name.to_lowercase();
                name.contains(&wanted) && name.starts_with(&wanted) == from_the_start
            })
        };
        named(true)
            .chain(named(false))
            .take(MOST_APPS)
            .map(|(app, _)| app.clone())
            .collect()
    }

    pub fn picks(&self) -> Vec<Pick> {
        let windows = self.windows().into_iter().map(Pick::Window);
        windows.chain(self.apps().into_iter().map(Pick::App)).collect()
    }

    pub fn selected(&self) -> Option<Pick> {
        self.picks().into_iter().nth(self.selected)
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn select(&mut self, index: usize) {
        if index < self.picks().len() {
            self.selected = index;
        }
    }

    pub fn numbered(&self, number: usize) -> Option<Pick> {
        let window = *self.windows().get(number.checked_sub(1)?)?;
        Some(Pick::Window(window))
    }

    pub fn step(&mut self, by: isize) {
        let count = self.picks().len() as isize;
        if count > 0 {
            self.selected = (self.selected as isize + by).rem_euclid(count) as usize;
        }
    }

    pub fn forget(&mut self, window: WindowId) {
        self.windows.retain(|(id, _)| *id != window);
        self.selected = self.selected.min(self.picks().len().saturating_sub(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overview() -> Overview {
        Overview::open(
            vec![
                (WindowId(1), "Inbox — Mail".to_owned()),
                (WindowId(2), "notes.txt — Editor".to_owned()),
                (WindowId(3), "Terminal".to_owned()),
            ],
            vec![
                (AppId::new("calculator"), "Calculator".to_owned()),
                (AppId::new("editor"), "Text Editor".to_owned()),
                (AppId::new("mail"), "Mail".to_owned()),
                (AppId::new("terminal"), "Terminal".to_owned()),
            ],
        )
    }

    #[test]
    fn it_opens_on_every_window_with_the_most_recent_one_selected() {
        let overview = overview();

        assert_eq!(overview.windows(), [WindowId(1), WindowId(2), WindowId(3)]);
        assert_eq!(overview.apps(), []);
        assert_eq!(overview.selected(), Some(Pick::Window(WindowId(1))));
    }

    #[test]
    fn typing_narrows_the_windows_and_brings_up_matching_apps() {
        let mut overview = overview();

        overview.typed("edit");

        assert_eq!(overview.windows(), [WindowId(2)]);
        assert_eq!(overview.apps(), [AppId::new("editor")]);
        assert_eq!(overview.selected(), Some(Pick::Window(WindowId(2))));
    }

    #[test]
    fn the_search_does_not_care_about_case() {
        let mut overview = overview();

        overview.typed("MAIL");

        assert_eq!(overview.windows(), [WindowId(1)]);
        assert_eq!(overview.apps(), [AppId::new("mail")]);
    }

    #[test]
    fn when_no_window_matches_the_first_matching_app_is_selected() {
        let mut overview = overview();

        overview.typed("calc");

        assert_eq!(overview.selected(), Some(Pick::App(AppId::new("calculator"))));
    }

    #[test]
    fn apps_whose_name_starts_with_the_search_come_before_those_that_merely_contain_it() {
        let mut overview = Overview::open(
            Vec::new(),
            vec![
                (AppId::new("a"), "Solitaire".to_owned()),
                (AppId::new("b"), "Terminal".to_owned()),
            ],
        );

        overview.typed("t");

        assert_eq!(overview.apps(), [AppId::new("b"), AppId::new("a")]);
    }

    #[test]
    fn no_more_apps_are_offered_than_fit_in_a_row() {
        let many = (0..30)
            .map(|number| (AppId::new(format!("app{number}")), format!("App {number}")))
            .collect();
        let mut overview = Overview::open(Vec::new(), many);

        overview.typed("app");

        assert_eq!(overview.apps().len(), MOST_APPS);
    }

    #[test]
    fn erasing_widens_the_search_again() {
        let mut overview = overview();
        overview.typed("term");

        overview.erased();
        overview.erased();
        overview.erased();
        overview.erased();

        assert_eq!(overview.query(), "");
        assert_eq!(overview.windows().len(), 3);
    }

    #[test]
    fn the_selection_walks_through_windows_then_apps_and_wraps_around() {
        let mut overview = overview();
        overview.typed("te");

        overview.step(1);
        assert_eq!(overview.selected(), Some(Pick::Window(WindowId(3))));
        overview.step(1);
        assert_eq!(overview.selected(), Some(Pick::App(AppId::new("editor"))));
        overview.step(2);
        assert_eq!(overview.selected(), Some(Pick::Window(WindowId(2))));
        overview.step(-1);
        assert_eq!(overview.selected(), Some(Pick::App(AppId::new("terminal"))));
    }

    #[test]
    fn windows_are_numbered_from_one_in_the_order_they_are_shown() {
        let mut overview = overview();

        assert_eq!(overview.numbered(1), Some(Pick::Window(WindowId(1))));
        assert_eq!(overview.numbered(3), Some(Pick::Window(WindowId(3))));
        assert_eq!(overview.numbered(4), None);
        assert_eq!(overview.numbered(0), None);

        overview.typed("term");
        assert_eq!(overview.numbered(1), Some(Pick::Window(WindowId(3))));
    }

    #[test]
    fn pointing_at_something_selects_it() {
        let mut overview = overview();

        overview.select(2);
        overview.select(99);

        assert_eq!(overview.selected(), Some(Pick::Window(WindowId(3))));
    }

    #[test]
    fn a_window_that_closes_drops_out_and_the_selection_stays_valid() {
        let mut overview = overview();
        overview.select(2);

        overview.forget(WindowId(3));

        assert_eq!(overview.windows(), [WindowId(1), WindowId(2)]);
        assert_eq!(overview.selected(), Some(Pick::Window(WindowId(2))));
    }

    #[test]
    fn typed_control_characters_are_not_part_of_the_search() {
        let mut overview = overview();

        overview.typed("\u{1b}t\r");

        assert_eq!(overview.query(), "t");
    }

    #[test]
    fn nothing_is_selected_when_nothing_matches() {
        let mut overview = overview();

        overview.typed("zzzz");

        assert_eq!(overview.selected(), None);
        overview.step(1);
        assert_eq!(overview.selected(), None);
    }
}
