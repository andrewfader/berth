#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AppId(String);

impl AppId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub id: WindowId,
    pub app: AppId,
    pub title: String,
    pub active: bool,
    pub minimized: bool,
}

#[derive(Clone, Default)]
pub struct Windows {
    most_recent_first: Vec<Window>,
}

impl Windows {
    pub fn report(&mut self, window: Window) {
        let known = self.position(window.id);
        let gained_focus = window.active && known.is_none_or(|at| !self.most_recent_first[at].active);
        match known {
            Some(at) if gained_focus => {
                self.most_recent_first.remove(at);
                self.most_recent_first.insert(0, window);
            }
            Some(at) => self.most_recent_first[at] = window,
            None if gained_focus => self.most_recent_first.insert(0, window),
            None => self.most_recent_first.push(window),
        }
    }

    pub fn closed(&mut self, id: WindowId) {
        self.most_recent_first.retain(|window| window.id != id);
    }

    pub fn most_recent_first(&self) -> &[Window] {
        &self.most_recent_first
    }

    pub fn get(&self, id: WindowId) -> Option<&Window> {
        self.most_recent_first.iter().find(|window| window.id == id)
    }

    pub fn active(&self) -> Option<&Window> {
        self.most_recent_first.iter().find(|window| window.active)
    }

    pub fn is_empty(&self) -> bool {
        self.most_recent_first.is_empty()
    }

    fn position(&self, id: WindowId) -> Option<usize> {
        self.most_recent_first.iter().position(|window| window.id == id)
    }
}

pub fn listing(windows: &Windows) -> String {
    windows
        .most_recent_first()
        .iter()
        .map(|window| {
            let state = match (window.active, window.minimized) {
                (true, _) => "focused",
                (false, true) => "minimized",
                (false, false) => "open",
            };
            format!("{state}\t{}\t{}\n", window.app.as_str(), window.title)
        })
        .collect()
}

#[cfg(test)]
pub mod fixtures {
    use super::*;

    pub fn window(id: u32, app: &str) -> Window {
        Window {
            id: WindowId(id),
            app: AppId::new(app),
            title: format!("{app} {id}"),
            active: false,
            minimized: false,
        }
    }

    pub fn focused(id: u32, app: &str) -> Window {
        Window {
            active: true,
            ..window(id, app)
        }
    }

    pub fn minimized(id: u32, app: &str) -> Window {
        Window {
            minimized: true,
            ..window(id, app)
        }
    }

    pub fn used_in_order(windows: &[Window]) -> Windows {
        let mut all = Windows::default();
        for window in windows.iter().rev() {
            all.report(Window {
                active: true,
                ..window.clone()
            });
            all.report(Window {
                active: false,
                ..window.clone()
            });
        }
        if let Some(front) = windows.first() {
            all.report(front.clone());
        }
        all
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn recency(windows: &Windows) -> Vec<u32> {
        windows
            .most_recent_first()
            .iter()
            .map(|window| window.id.0)
            .collect()
    }

    #[test]
    fn the_window_that_just_gained_focus_becomes_the_most_recent() {
        let mut windows = Windows::default();
        windows.report(focused(1, "editor"));
        windows.report(window(1, "editor"));
        windows.report(focused(2, "browser"));
        windows.report(window(2, "browser"));

        windows.report(focused(1, "editor"));

        assert_eq!(recency(&windows), [1, 2]);
    }

    #[test]
    fn a_window_opened_in_the_background_queues_behind_the_ones_already_used() {
        let mut windows = Windows::default();
        windows.report(focused(1, "editor"));

        windows.report(window(2, "browser"));

        assert_eq!(recency(&windows), [1, 2]);
    }

    #[test]
    fn a_retitled_window_keeps_its_place() {
        let mut windows = used_in_order(&[focused(1, "editor"), window(2, "browser")]);

        windows.report(Window {
            title: "renamed".into(),
            ..window(2, "browser")
        });

        assert_eq!(recency(&windows), [1, 2]);
        assert_eq!(windows.get(WindowId(2)).unwrap().title, "renamed");
    }

    #[test]
    fn a_window_that_stays_focused_while_changing_is_not_moved_again() {
        let mut windows = Windows::default();
        windows.report(focused(1, "editor"));
        windows.report(window(2, "browser"));

        windows.report(Window {
            title: "still here".into(),
            ..focused(1, "editor")
        });

        assert_eq!(recency(&windows), [1, 2]);
    }

    #[test]
    fn the_listing_names_each_window_with_its_state_most_recent_first() {
        let windows = used_in_order(&[
            focused(1, "editor"),
            minimized(2, "browser"),
            window(3, "terminal"),
        ]);

        assert_eq!(
            listing(&windows),
            "focused\teditor\teditor 1\nminimized\tbrowser\tbrowser 2\nopen\tterminal\tterminal 3\n"
        );
    }

    #[test]
    fn a_closed_window_is_forgotten() {
        let mut windows = used_in_order(&[focused(1, "editor"), window(2, "browser")]);

        windows.closed(WindowId(1));

        assert_eq!(recency(&windows), [2]);
        assert!(windows.active().is_none());
    }
}
