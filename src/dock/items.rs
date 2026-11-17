use crate::windows::{AppId, WindowId, Windows};

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
        windows.active().map(|window| window.id).filter(|id| self.oldest_first.contains(id))
    }

    pub fn most_recent_window(&self, windows: &Windows) -> Option<WindowId> {
        windows
            .most_recent_first()
            .iter()
            .map(|window| window.id)
            .find(|id| self.oldest_first.contains(id))
    }
}

pub fn dock_items(pinned: &[AppId], windows: &Windows) -> Vec<DockItem> {
    let mut items: Vec<DockItem> = pinned
        .iter()
        .map(|app| DockItem { app: app.clone(), pinned: true, oldest_first: Vec::new() })
        .collect();

    let mut oldest_first: Vec<_> = windows.most_recent_first().iter().collect();
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

        let items = dock_items(&pinned, &windows);

        assert_eq!(apps(&items), ["browser", "editor", "terminal"]);
    }

    #[test]
    fn a_pinned_app_without_windows_is_shown_as_not_running() {
        let items = dock_items(&[AppId::new("browser")], &Windows::default());

        assert!(items[0].pinned);
        assert!(!items[0].is_running());
    }

    #[test]
    fn all_windows_of_one_app_share_a_single_item() {
        let windows = used_in_order(&[focused(3, "editor"), window(1, "editor"), window(2, "browser")]);

        let items = dock_items(&[], &windows);

        assert_eq!(apps(&items), ["editor", "browser"]);
        assert_eq!(items[0].oldest_first, [WindowId(1), WindowId(3)]);
    }

    #[test]
    fn running_apps_keep_their_place_when_focus_moves_between_them() {
        let mut windows = used_in_order(&[focused(1, "editor"), window(2, "browser")]);
        let before = apps(&dock_items(&[], &windows)).join(" ");

        windows.report(focused(2, "browser"));

        assert_eq!(apps(&dock_items(&[], &windows)).join(" "), before);
    }

    #[test]
    fn an_item_knows_which_of_its_windows_was_used_last() {
        let windows = used_in_order(&[focused(9, "browser"), window(3, "editor"), window(1, "editor")]);

        let items = dock_items(&[], &windows);
        let editor = items.iter().find(|item| item.app.as_str() == "editor").unwrap();

        assert_eq!(editor.most_recent_window(&windows), Some(WindowId(3)));
        assert_eq!(editor.active_window(&windows), None);
    }
}
