use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::windows::AppId;

pub struct Pins {
    apps: Vec<AppId>,
    store: Option<PathBuf>,
}

impl Pins {
    pub fn fixed(apps: Vec<AppId>) -> Self {
        Self { apps, store: None }
    }

    pub fn kept_in(store: PathBuf, apps: Vec<AppId>) -> Self {
        Self {
            apps,
            store: Some(store),
        }
    }

    pub fn remembered(store: &Path) -> Option<Vec<String>> {
        let text = fs::read_to_string(store).ok()?;
        Some(
            text.lines()
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect(),
        )
    }

    pub fn apps(&self) -> &[AppId] {
        &self.apps
    }

    pub fn are_editable(&self) -> bool {
        self.store.is_some()
    }

    pub fn pin(&mut self, app: AppId) -> io::Result<()> {
        let mut apps = self.apps.clone();
        if !apps.contains(&app) {
            apps.push(app);
        }
        self.remember(apps)
    }

    pub fn unpin(&mut self, app: &AppId) -> io::Result<()> {
        let apps = self
            .apps
            .iter()
            .filter(|pinned| *pinned != app)
            .cloned()
            .collect();
        self.remember(apps)
    }

    fn remember(&mut self, apps: Vec<AppId>) -> io::Result<()> {
        let Some(store) = &self.store else {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the pinned apps are set in the configuration file",
            ));
        };
        let listing: String = apps.iter().map(|app| format!("{}\n", app.as_str())).collect();
        if let Some(folder) = store.parent() {
            fs::create_dir_all(folder)?;
        }
        let draft = store.with_extension("new");
        fs::write(&draft, listing)?;
        fs::rename(draft, store)?;
        self.apps = apps;
        Ok(())
    }
}

pub fn store() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"))
        .join("berth/pinned")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str) -> AppId {
        AppId::new(id)
    }

    #[test]
    fn a_pin_made_from_the_dock_is_still_there_after_a_restart() {
        let home = tempfile::tempdir().unwrap();
        let store = home.path().join("berth/pinned");
        let mut pins = Pins::kept_in(store.clone(), vec![app("browser")]);

        pins.pin(app("editor")).unwrap();

        assert_eq!(pins.apps(), [app("browser"), app("editor")]);
        assert_eq!(Pins::remembered(&store).unwrap(), ["browser", "editor"]);
    }

    #[test]
    fn unpinning_takes_the_app_off_the_dock_for_good() {
        let home = tempfile::tempdir().unwrap();
        let store = home.path().join("pinned");
        let mut pins = Pins::kept_in(store.clone(), vec![app("browser"), app("editor")]);

        pins.unpin(&app("browser")).unwrap();

        assert_eq!(Pins::remembered(&store).unwrap(), ["editor"]);
    }

    #[test]
    fn pinning_the_same_app_twice_keeps_one_pin() {
        let home = tempfile::tempdir().unwrap();
        let mut pins = Pins::kept_in(home.path().join("pinned"), vec![app("browser")]);

        pins.pin(app("browser")).unwrap();

        assert_eq!(pins.apps(), [app("browser")]);
    }

    #[test]
    fn nothing_is_remembered_before_the_first_change() {
        let home = tempfile::tempdir().unwrap();

        assert_eq!(Pins::remembered(&home.path().join("pinned")), None);
    }

    #[test]
    fn pins_fixed_by_the_configuration_cannot_be_changed_from_the_dock() {
        let mut pins = Pins::fixed(vec![app("browser")]);

        assert!(!pins.are_editable());
        assert!(pins.pin(app("editor")).is_err());
        assert_eq!(pins.apps(), [app("browser")]);
    }

    #[test]
    fn a_pin_that_cannot_be_saved_is_reported() {
        let home = tempfile::tempdir().unwrap();
        let blocked = home.path().join("file");
        fs::write(&blocked, "").unwrap();
        let mut pins = Pins::kept_in(blocked.join("pinned"), Vec::new());

        assert!(pins.pin(app("editor")).is_err());
    }
}
