use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;

pub const FAVORITES: &str = "com.system76.CosmicAppList/v1/favorites";
pub const ICON_THEME: &str = "com.system76.CosmicTk/v1/icon_theme";

pub fn setting<T: DeserializeOwned>(config_home: &Path, key: &str) -> Option<T> {
    let text = fs::read_to_string(config_home.join("cosmic").join(key)).ok()?;
    ron::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desktop_with(key: &str, value: &str) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        let file = home.path().join("cosmic").join(key);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, value).unwrap();
        home
    }

    #[test]
    fn the_apps_pinned_to_the_cosmic_dock_are_read_in_order() {
        let home = desktop_with(FAVORITES, "[\n    \"firefox\",\n    \"org.gnome.Nautilus\",\n]");

        let favorites: Option<Vec<String>> = setting(home.path(), FAVORITES);

        assert_eq!(favorites.unwrap(), ["firefox", "org.gnome.Nautilus"]);
    }

    #[test]
    fn the_icon_theme_chosen_in_cosmic_settings_is_read() {
        let home = desktop_with(ICON_THEME, "\"Cosmic\"");

        assert_eq!(setting::<String>(home.path(), ICON_THEME).unwrap(), "Cosmic");
    }

    #[test]
    fn a_setting_that_is_absent_or_unreadable_is_simply_not_set() {
        let home = desktop_with(FAVORITES, "not a list");

        assert_eq!(setting::<Vec<String>>(home.path(), FAVORITES), None);
        assert_eq!(setting::<String>(home.path(), ICON_THEME), None);
    }
}
