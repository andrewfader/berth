use std::io;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;

use freedesktop_desktop_entry::unicase::Ascii;
use freedesktop_desktop_entry::{DesktopEntry, Iter, default_paths, find_app_by_id, get_languages_from_env};

use crate::windows::AppId;

pub struct Catalog {
    entries: Vec<DesktopEntry>,
    locales: Vec<String>,
}

impl Catalog {
    pub fn installed() -> Self {
        Self::found_in(default_paths())
    }

    pub fn found_in(directories: impl Iterator<Item = PathBuf>) -> Self {
        let locales = get_languages_from_env();
        let entries = Iter::new(directories)
            .filter_map(|path| DesktopEntry::from_path(path, Some(&locales)).ok())
            .collect();
        Self { entries, locales }
    }

    pub fn identify(&self, window_app_id: &str) -> AppId {
        match self.entry(window_app_id) {
            Some(entry) => AppId::new(entry.id()),
            None => AppId::new(window_app_id),
        }
    }

    pub fn name(&self, app: &AppId) -> String {
        self.entry(app.as_str())
            .and_then(|entry| entry.name(&self.locales))
            .map_or_else(|| app.as_str().to_owned(), |name| name.into_owned())
    }

    pub fn icon(&self, app: &AppId) -> Option<&str> {
        self.entry(app.as_str())?.icon()
    }

    pub fn pinned(&self, wanted: &[String]) -> Vec<AppId> {
        let mut pinned = Vec::new();
        for id in wanted {
            match self.entry(id).map(|entry| AppId::new(entry.id())) {
                Some(app) if !pinned.contains(&app) => pinned.push(app),
                Some(_) => {}
                None => log::warn!("{id} is pinned but has no launcher, leaving it out"),
            }
        }
        pinned
    }

    pub fn command(&self, app: &AppId) -> Option<Vec<String>> {
        self.entry(app.as_str())?
            .parse_exec()
            .ok()
            .filter(|words| !words.is_empty())
    }

    fn entry(&self, id: &str) -> Option<&DesktopEntry> {
        find_app_by_id(&self.entries, Ascii::new(id))
    }
}

pub fn launch(command: &[String]) -> io::Result<()> {
    let (program, arguments) = command.split_first().ok_or(io::ErrorKind::InvalidInput)?;
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, Instant};

    fn catalog_of(entries: &[(&str, &str)]) -> (tempfile::TempDir, Catalog) {
        let dir = tempfile::tempdir().unwrap();
        for (file, body) in entries {
            fs::write(
                dir.path().join(file),
                format!("[Desktop Entry]\nType=Application\n{body}\n"),
            )
            .unwrap();
        }
        let catalog = Catalog::found_in([dir.path().to_owned()].into_iter());
        (dir, catalog)
    }

    #[test]
    fn a_window_is_matched_to_its_launcher_by_desktop_file_name_whatever_the_case() {
        let (_dir, catalog) = catalog_of(&[("org.example.Editor.desktop", "Name=Editor\nExec=editor")]);

        assert_eq!(
            catalog.identify("org.example.editor"),
            AppId::new("org.example.Editor")
        );
    }

    #[test]
    fn a_window_whose_app_id_differs_is_matched_by_its_startup_wm_class() {
        let (_dir, catalog) = catalog_of(&[(
            "browser-nightly.desktop",
            "Name=Browser\nExec=browser\nStartupWMClass=browser-dev",
        )]);

        assert_eq!(catalog.identify("browser-dev"), AppId::new("browser-nightly"));
    }

    #[test]
    fn a_window_with_no_launcher_keeps_its_own_app_id_and_is_named_after_it() {
        let (_dir, catalog) = catalog_of(&[]);

        let app = catalog.identify("mystery-tool");

        assert_eq!(app, AppId::new("mystery-tool"));
        assert_eq!(catalog.name(&app), "mystery-tool");
        assert_eq!(catalog.icon(&app), None);
        assert_eq!(catalog.command(&app), None);
    }

    #[test]
    fn an_app_is_described_by_its_launcher() {
        let (_dir, catalog) = catalog_of(&[(
            "editor.desktop",
            "Name=Text Editor\nIcon=accessories-text-editor\nExec=editor",
        )]);
        let app = AppId::new("editor");

        assert_eq!(catalog.name(&app), "Text Editor");
        assert_eq!(catalog.icon(&app), Some("accessories-text-editor"));
    }

    #[test]
    fn the_launch_command_leaves_out_the_file_placeholders() {
        let (_dir, catalog) = catalog_of(&[("editor.desktop", "Name=Editor\nExec=editor --new-window %U")]);

        assert_eq!(
            catalog.command(&AppId::new("editor")).unwrap(),
            ["editor", "--new-window"]
        );
    }

    #[test]
    fn pins_keep_their_order_under_the_name_of_their_launcher_without_repeats() {
        let (_dir, catalog) = catalog_of(&[
            ("org.example.Editor.desktop", "Name=Editor\nExec=editor"),
            ("browser.desktop", "Name=Browser\nExec=browser"),
        ]);
        let wanted = ["browser", "org.example.editor", "Browser"].map(String::from);

        assert_eq!(
            catalog.pinned(&wanted),
            [AppId::new("browser"), AppId::new("org.example.Editor")]
        );
    }

    #[test]
    fn a_pin_for_an_app_that_is_not_installed_is_left_out() {
        let (_dir, catalog) = catalog_of(&[("browser.desktop", "Name=Browser\nExec=browser")]);
        let wanted = ["uninstalled-app", "browser"].map(String::from);

        assert_eq!(catalog.pinned(&wanted), [AppId::new("browser")]);
    }

    #[test]
    fn launching_starts_the_program_with_its_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("started");

        launch(&["touch".into(), marker.to_string_lossy().into_owned()]).unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while !marker.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists());
    }

    #[test]
    fn a_program_that_cannot_be_started_is_reported() {
        assert!(launch(&["/nonexistent/berth-test-program".into()]).is_err());
        assert!(launch(&[]).is_err());
    }
}
