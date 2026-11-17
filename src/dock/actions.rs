use serde::Deserialize;

use super::items::DockItem;
use crate::windows::{AppId, WindowId, Windows};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Click,
    MiddleClick,
    RightClick,
    ShiftClick,
    CtrlClick,
    ScrollUp,
    ScrollDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Launch,
    Focus,
    Toggle,
    Cycle,
    CycleBack,
    Minimize,
    Close,
    Quit,
    Menu,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Launch(AppId),
    Activate(WindowId),
    Minimize(WindowId),
    Close(WindowId),
    Pin(AppId),
    Unpin(AppId),
}

pub fn effects(action: Action, item: &DockItem, windows: &Windows) -> Vec<Effect> {
    let launch = || vec![Effect::Launch(item.app.clone())];
    let most_recent = item.most_recent_window(windows);
    let active = item.active_window(windows);

    match (action, most_recent) {
        (Action::None | Action::Menu, _) => Vec::new(),
        (Action::Launch, _) => launch(),
        (Action::Focus | Action::Toggle | Action::Cycle | Action::CycleBack, None) => launch(),
        (Action::Minimize | Action::Close | Action::Quit, None) => Vec::new(),
        (Action::Focus, Some(window)) => vec![Effect::Activate(window)],
        (Action::Toggle, Some(window)) => match active {
            Some(active) => vec![Effect::Minimize(active)],
            None => vec![Effect::Activate(window)],
        },
        (Action::Cycle, Some(window)) => vec![Effect::Activate(neighbour(item, active, 1).unwrap_or(window))],
        (Action::CycleBack, Some(window)) => {
            vec![Effect::Activate(neighbour(item, active, -1).unwrap_or(window))]
        }
        (Action::Minimize, Some(_)) => item
            .oldest_first
            .iter()
            .filter(|id| windows.get(**id).is_some_and(|window| !window.minimized))
            .map(|id| Effect::Minimize(*id))
            .collect(),
        (Action::Close, Some(window)) => vec![Effect::Close(window)],
        (Action::Quit, Some(_)) => item.oldest_first.iter().map(|id| Effect::Close(*id)).collect(),
    }
}

fn neighbour(item: &DockItem, active: Option<WindowId>, step: isize) -> Option<WindowId> {
    let count = item.oldest_first.len() as isize;
    let at = item.oldest_first.iter().position(|id| Some(*id) == active)? as isize;
    Some(item.oldest_first[(at + step).rem_euclid(count) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::items::dock_items;
    use crate::windows::fixtures::*;

    fn on_app(app: &str, action: Action, windows: &Windows) -> Vec<Effect> {
        let items = dock_items(&[AppId::new(app)], windows);
        effects(action, &items[0], windows)
    }

    fn launch(app: &str) -> Vec<Effect> {
        vec![Effect::Launch(AppId::new(app))]
    }

    #[test]
    fn every_window_action_starts_an_app_that_is_not_running() {
        let nothing_open = Windows::default();

        for action in [
            Action::Launch,
            Action::Focus,
            Action::Toggle,
            Action::Cycle,
            Action::CycleBack,
        ] {
            assert_eq!(
                on_app("editor", action, &nothing_open),
                launch("editor"),
                "{action:?}"
            );
        }
    }

    #[test]
    fn there_is_nothing_to_minimize_or_close_for_an_app_that_is_not_running() {
        let nothing_open = Windows::default();

        for action in [
            Action::Minimize,
            Action::Close,
            Action::Quit,
            Action::Menu,
            Action::None,
        ] {
            assert_eq!(on_app("editor", action, &nothing_open), [], "{action:?}");
        }
    }

    #[test]
    fn launch_starts_another_instance_of_a_running_app() {
        let windows = used_in_order(&[focused(1, "editor")]);

        assert_eq!(on_app("editor", Action::Launch, &windows), launch("editor"));
    }

    #[test]
    fn focus_raises_the_window_of_the_app_that_was_used_last() {
        let windows = used_in_order(&[focused(9, "browser"), window(2, "editor"), window(1, "editor")]);

        assert_eq!(
            on_app("editor", Action::Focus, &windows),
            [Effect::Activate(WindowId(2))]
        );
    }

    #[test]
    fn toggle_raises_an_app_that_is_in_the_background() {
        let windows = used_in_order(&[focused(9, "browser"), minimized(1, "editor")]);

        assert_eq!(
            on_app("editor", Action::Toggle, &windows),
            [Effect::Activate(WindowId(1))]
        );
    }

    #[test]
    fn toggle_minimizes_the_window_that_already_has_focus() {
        let windows = used_in_order(&[focused(1, "editor"), window(9, "browser")]);

        assert_eq!(
            on_app("editor", Action::Toggle, &windows),
            [Effect::Minimize(WindowId(1))]
        );
    }

    #[test]
    fn cycle_walks_through_the_windows_of_the_focused_app_and_wraps_around() {
        let windows = used_in_order(&[focused(3, "editor"), window(1, "editor"), window(2, "editor")]);

        assert_eq!(
            on_app("editor", Action::Cycle, &windows),
            [Effect::Activate(WindowId(1))]
        );
        assert_eq!(
            on_app("editor", Action::CycleBack, &windows),
            [Effect::Activate(WindowId(2))]
        );
    }

    #[test]
    fn cycle_first_returns_to_the_last_used_window_of_a_background_app() {
        let windows = used_in_order(&[focused(9, "browser"), window(2, "editor"), window(1, "editor")]);

        assert_eq!(
            on_app("editor", Action::Cycle, &windows),
            [Effect::Activate(WindowId(2))]
        );
    }

    #[test]
    fn minimize_hides_every_window_of_the_app_that_is_still_visible() {
        let windows = used_in_order(&[focused(1, "editor"), minimized(2, "editor"), window(3, "editor")]);

        assert_eq!(
            on_app("editor", Action::Minimize, &windows),
            [Effect::Minimize(WindowId(1)), Effect::Minimize(WindowId(3))]
        );
    }

    #[test]
    fn close_closes_only_the_last_used_window_while_quit_closes_them_all() {
        let windows = used_in_order(&[focused(2, "editor"), window(1, "editor")]);

        assert_eq!(
            on_app("editor", Action::Close, &windows),
            [Effect::Close(WindowId(2))]
        );
        assert_eq!(
            on_app("editor", Action::Quit, &windows),
            [Effect::Close(WindowId(1)), Effect::Close(WindowId(2))]
        );
    }
}
