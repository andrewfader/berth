use serde::Deserialize;

use crate::windows::{AppId, WindowId, Windows};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shown {
    #[default]
    All,
    ThisMonitor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockItem {
    pub app: AppId,
    pub pinned: bool,
    pub oldest_first: Vec<WindowId>,
}

impl DockItem {
    pub fn is_running(&self) -> bool {
        !self.oldest_first.is_empty()
    }

    pub fn active_window(&self, windows: &Windows) -> Option<WindowId> {
        windows
            .active()
            .map(|window| window.id)
            .filter(|id| self.oldest_first.contains(id))
    }

    pub fn most_recent_window(&self, windows: &Windows) -> Option<WindowId> {
        windows
            .most_recent_first()
            .iter()
            .map(|window| window.id)
            .find(|id| self.oldest_first.contains(id))
    }
}

pub fn dock_items(pinned: &[AppId], windows: &Windows, monitor: Option<&str>) -> Vec<DockItem> {
    let mut items: Vec<DockItem> = pinned
        .iter()
        .map(|app| DockItem {
            app: app.clone(),
            pinned: true,
            oldest_first: Vec::new(),
        })
        .collect();

    let mut oldest_first: Vec<_> = windows
        .most_recent_first()
        .iter()
        .filter(|window| monitor.is_none_or(|monitor| window.is_on(monitor)))
        .collect();
    oldest_first.sort_by_key(|window| window.id);

    for window in oldest_first {
        match items.iter_mut().find(|item| item.app == window.app) {
            Some(item) => item.oldest_first.push(window.id),
            None => items.push(DockItem {
                app: window.app.clone(),
                pinned: false,
                oldest_first: vec![window.id],
            }),
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows::fixtures::*;

    fn apps(items: &[DockItem]) -> Vec<&str> {
        items.iter().map(|item| item.app.as_str()).collect()
    }

    #[test]
    fn pinned_apps_come_first_in_the_order_they_were_pinned() {
        let pinned = [AppId::new("browser"), AppId::new("editor")];
        let windows = used_in_order(&[focused(1, "terminal"), window(2, "editor")]);

        let items = dock_items(&pinned, &windows, None);

        assert_eq!(apps(&items), ["browser", "editor", "terminal"]);
    }

    #[test]
    fn a_pinned_app_without_windows_is_shown_as_not_running() {
        let items = dock_items(&[AppId::new("browser")], &Windows::default(), None);

        assert!(items[0].pinned);
        assert!(!items[0].is_running());
    }

    #[test]
    fn all_windows_of_one_app_share_a_single_item() {
        let windows = used_in_order(&[focused(3, "editor"), window(1, "editor"), window(2, "browser")]);

        let items = dock_items(&[], &windows, None);

        assert_eq!(apps(&items), ["editor", "browser"]);
        assert_eq!(items[0].oldest_first, [WindowId(1), WindowId(3)]);
    }

    #[test]
    fn running_apps_keep_their_place_when_focus_moves_between_them() {
        let mut windows = used_in_order(&[focused(1, "editor"), window(2, "browser")]);
        let before = apps(&dock_items(&[], &windows, None)).join(" ");

        windows.report(focused(2, "browser"));

        assert_eq!(apps(&dock_items(&[], &windows, None)).join(" "), before);
    }

    #[test]
    fn a_dock_showing_only_its_own_monitor_leaves_out_windows_on_the_others() {
        let windows = used_in_order(&[
            on_monitor("left", focused(1, "editor")),
            on_monitor("right", window(2, "browser")),
            on_monitor("right", window(3, "editor")),
        ]);

        let items = dock_items(&[AppId::new("browser")], &windows, Some("left"));

        assert_eq!(apps(&items), ["browser", "editor"]);
        assert!(!items[0].is_running());
        assert_eq!(items[1].oldest_first, [WindowId(1)]);
    }

    #[test]
    fn a_window_whose_monitor_is_unknown_is_shown_everywhere() {
        let windows = used_in_order(&[focused(1, "editor")]);

        let items = dock_items(&[], &windows, Some("left"));

        assert_eq!(apps(&items), ["editor"]);
    }

    #[test]
    fn an_item_knows_which_of_its_windows_was_used_last() {
        let windows = used_in_order(&[focused(9, "browser"), window(3, "editor"), window(1, "editor")]);

        let items = dock_items(&[], &windows, None);
        let editor = items.iter().find(|item| item.app.as_str() == "editor").unwrap();

        assert_eq!(editor.most_recent_window(&windows), Some(WindowId(3)));
        assert_eq!(editor.active_window(&windows), None);
    }
}
