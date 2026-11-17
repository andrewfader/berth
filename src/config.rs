use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::dock::actions::{Action, Gesture};
use crate::dock::reveal::Autohide;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub dock: Dock,
    pub switcher: Switcher,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Dock {
    pub icon_size: u32,
    pub magnification: f32,
    pub opacity: f32,
    pub blur: bool,
    pub autohide: Autohide,
    pub pinned: Option<Vec<String>>,
    pub actions: Gestures,
}

impl Default for Dock {
    fn default() -> Self {
        Self {
            icon_size: 48,
            magnification: 1.6,
            opacity: 0.55,
            blur: true,
            autohide: Autohide::Intelligent,
            pinned: None,
            actions: Gestures::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Gestures {
    pub click: Action,
    pub middle_click: Action,
    pub right_click: Action,
    pub scroll_up: Action,
    pub scroll_down: Action,
}

impl Default for Gestures {
    fn default() -> Self {
        Self {
            click: Action::Toggle,
            middle_click: Action::Launch,
            right_click: Action::Minimize,
            scroll_up: Action::CycleBack,
            scroll_down: Action::Cycle,
        }
    }
}

impl Gestures {
    pub fn action_for(&self, gesture: Gesture) -> Action {
        match gesture {
            Gesture::Click => self.click,
            Gesture::MiddleClick => self.middle_click,
            Gesture::RightClick => self.right_click,
            Gesture::ScrollUp => self.scroll_up,
            Gesture::ScrollDown => self.scroll_down,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Switcher {
    pub opacity: f32,
    pub blur: bool,
}

impl Default for Switcher {
    fn default() -> Self {
        Self {
            opacity: 0.45,
            blur: true,
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Unreadable(PathBuf, io::Error),
    Malformed(toml::de::Error),
    OutOfRange {
        key: &'static str,
        allowed: &'static str,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Unreadable(path, error) => write!(f, "cannot read {}: {error}", path.display()),
            ConfigError::Malformed(error) => write!(f, "{error}"),
            ConfigError::OutOfRange { key, allowed } => write!(f, "{key} must be {allowed}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let config: Config = toml::from_str(text).map_err(ConfigError::Malformed)?;
        config.validated()
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(ConfigError::Unreadable(path.to_owned(), error)),
        }
    }

    pub fn path() -> PathBuf {
        config_home().join("berth/config.toml")
    }

    fn validated(self) -> Result<Self, ConfigError> {
        let within =
            |ok: bool, key, allowed| ok.then_some(()).ok_or(ConfigError::OutOfRange { key, allowed });
        within(
            (16..=256).contains(&self.dock.icon_size),
            "dock.icon-size",
            "between 16 and 256",
        )?;
        within(
            (1.0..=3.0).contains(&self.dock.magnification),
            "dock.magnification",
            "between 1.0 and 3.0",
        )?;
        within(
            (0.0..=1.0).contains(&self.dock.opacity),
            "dock.opacity",
            "between 0.0 and 1.0",
        )?;
        within(
            (0.0..=1.0).contains(&self.switcher.opacity),
            "switcher.opacity",
            "between 0.0 and 1.0",
        )?;
        Ok(self)
    }
}

pub fn config_home() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejection(text: &str) -> String {
        Config::parse(text).unwrap_err().to_string()
    }

    #[test]
    fn it_works_without_any_configuration() {
        let config = Config::parse("").unwrap();

        assert_eq!(config, Config::default());
        assert_eq!(config.dock.autohide, Autohide::Intelligent);
        assert!(config.dock.blur && config.dock.opacity < 1.0);
    }

    #[test]
    fn a_missing_file_means_defaults() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            Config::load(&dir.path().join("config.toml")).unwrap(),
            Config::default()
        );
    }

    #[test]
    fn the_look_of_the_dock_is_configurable() {
        let config = Config::parse(
            r#"
            [dock]
            icon-size = 64
            magnification = 2.0
            opacity = 0.3
            blur = false
            autohide = "always"
            pinned = ["firefox", "org.gnome.Nautilus"]
            "#,
        )
        .unwrap();

        assert_eq!(config.dock.icon_size, 64);
        assert_eq!(config.dock.magnification, 2.0);
        assert_eq!(config.dock.opacity, 0.3);
        assert!(!config.dock.blur);
        assert_eq!(config.dock.autohide, Autohide::Always);
        assert_eq!(config.dock.pinned.unwrap(), ["firefox", "org.gnome.Nautilus"]);
    }

    #[test]
    fn each_gesture_can_be_given_its_own_action_while_the_rest_keep_their_defaults() {
        let config = Config::parse(
            r#"
            [dock.actions]
            click = "focus"
            middle-click = "close"
            scroll-down = "none"
            "#,
        )
        .unwrap();

        assert_eq!(config.dock.actions.action_for(Gesture::Click), Action::Focus);
        assert_eq!(
            config.dock.actions.action_for(Gesture::MiddleClick),
            Action::Close
        );
        assert_eq!(config.dock.actions.action_for(Gesture::ScrollDown), Action::None);
        assert_eq!(
            config.dock.actions.action_for(Gesture::ScrollUp),
            Action::CycleBack
        );
        assert_eq!(
            config.dock.actions.action_for(Gesture::RightClick),
            Action::Minimize
        );
    }

    #[test]
    fn the_switcher_backdrop_is_configurable() {
        let config = Config::parse("[switcher]\nopacity = 0.8\nblur = false").unwrap();

        assert_eq!(
            config.switcher,
            Switcher {
                opacity: 0.8,
                blur: false
            }
        );
    }

    #[test]
    fn a_misspelled_key_is_reported_instead_of_silently_ignored() {
        assert!(rejection("[dock]\nicon_size = 64").contains("icon_size"));
    }

    #[test]
    fn an_unknown_action_is_reported_with_the_ones_that_exist() {
        let message = rejection("[dock.actions]\nclick = \"explode\"");

        assert!(
            message.contains("explode") && message.contains("toggle"),
            "{message}"
        );
    }

    #[test]
    fn values_outside_their_range_are_rejected_by_name() {
        assert_eq!(
            rejection("[dock]\nopacity = 1.5"),
            "dock.opacity must be between 0.0 and 1.0"
        );
        assert_eq!(
            rejection("[dock]\nmagnification = 0.5"),
            "dock.magnification must be between 1.0 and 3.0"
        );
        assert_eq!(
            rejection("[dock]\nicon-size = 4"),
            "dock.icon-size must be between 16 and 256"
        );
        assert_eq!(
            rejection("[switcher]\nopacity = -1"),
            "switcher.opacity must be between 0.0 and 1.0"
        );
    }

    #[test]
    fn an_unreadable_file_is_an_error_not_a_silent_fallback() {
        let dir = tempfile::tempdir().unwrap();

        let error = Config::load(dir.path()).unwrap_err();

        assert!(error.to_string().starts_with("cannot read "), "{error}");
    }
}
