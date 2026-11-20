use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use freedesktop_desktop_entry::unicase::Ascii;
use freedesktop_desktop_entry::{DesktopEntry, Iter, default_paths, find_app_by_id, get_languages_from_env};
use nix::sys::signal::SigSet;

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

    pub fn launchable(&self) -> Vec<(AppId, String)> {
        let mut apps: Vec<(AppId, String)> = self
            .entries
            .iter()
            .filter(|entry| !entry.no_display() && !entry.hidden() && entry.exec().is_some())
            .filter(|entry| entry.type_().is_none_or(|kind| kind == "Application"))
            .map(|entry| {
                let name = entry
                    .name(&self.locales)
                    .map_or_else(|| entry.id().to_owned(), |name| name.into_owned());
                (AppId::new(entry.id()), name)
            })
            .collect();
        apps.sort_by_key(|(app, name)| (name.to_lowercase(), app.clone()));
        apps.dedup_by(|later, earlier| later.0 == earlier.0);
        apps
    }

    pub fn command(&self, app: &AppId) -> Option<Vec<String>> {
        self.entry(app.as_str())?
            .parse_exec()
            .ok()
            .filter(|words| !words.is_empty())
    }

    fn entry(&self, id: &str) -> Option<&DesktopEntry> {
        let wanted = Ascii::new(id);
        self.entries
            .iter()
            .find(|entry| Ascii::new(entry.id()) == wanted)
            .or_else(|| find_app_by_id(&self.entries, wanted))
    }
}

pub fn launch(app: &AppId, command: &[String]) -> io::Result<()> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_default();
    launch_with(&runtime, app, command)
}

fn launch_with(runtime: &Path, app: &AppId, command: &[String]) -> io::Result<()> {
    let (program, arguments) = command.split_first().ok_or(io::ErrorKind::InvalidInput)?;
    let mut command = if runtime.join("systemd/private").exists() {
        let mut scoped = Command::new("systemd-run");
        scoped
            .args(["--user", "--scope", "--quiet", "--collect", "--slice=app.slice"])
            .arg(format!("--unit={}", scope_for(app)))
            .arg("--")
            .arg(program);
        scoped
    } else {
        Command::new(program)
    };
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    unsafe { command.pre_exec(|| Ok(SigSet::empty().thread_set_mask()?)) };
    let mut child = command.spawn()?;
    thread::spawn(move || child.wait());
    Ok(())
}

fn scope_for(app: &AppId) -> String {
    let start = RandomState::new().build_hasher().finish();
    format!("app-berth-{}-{start:016x}.scope", unit_escaped(app.as_str()))
}

fn unit_escaped(id: &str) -> String {
    id.bytes()
        .enumerate()
        .map(|(at, byte)| match byte {
            b'/' => "-".to_owned(),
            b'.' if at == 0 => format!("\\x{byte:02x}"),
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b':' | b'_' | b'.' => char::from(byte).to_string(),
            _ => format!("\\x{byte:02x}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, Instant};

    fn catalog_of(entries: &[(&str, &str)]) -> (tempfile::TempDir, Catalog) {
        let dir = tempfile::tempdir().unwrap();
        let applications = dir.path().join("applications");
        for (file, body) in entries {
            fs::create_dir_all(applications.join(file).parent().unwrap()).unwrap();
            fs::write(
                applications.join(file),
                format!("[Desktop Entry]\nType=Application\n{body}\n"),
            )
            .unwrap();
        }
        let catalog = Catalog::found_in([applications].into_iter());
        (dir, catalog)
    }

    #[test]
    fn an_app_is_not_mistaken_for_another_whose_file_merely_shares_its_name() {
        let home = tempfile::tempdir().unwrap();
        let (mine, shared) = (
            home.path().join("mine/applications"),
            home.path().join("shared/applications"),
        );
        fs::create_dir_all(mine.join("wine/Programs/Steam")).unwrap();
        fs::create_dir_all(&shared).unwrap();
        fs::write(
            mine.join("wine/Programs/Steam/Steam.desktop"),
            "[Desktop Entry]\nType=Application\nName=Steam\nExec=wine steam.exe\nStartupWMClass=steam.exe\n",
        )
        .unwrap();
        fs::write(
            shared.join("steam.desktop"),
            "[Desktop Entry]\nType=Application\nName=Steam\nExec=steam\n",
        )
        .unwrap();
        let catalog = Catalog::found_in([mine, shared].into_iter());

        assert_eq!(catalog.identify("steam"), AppId::new("steam"));
        assert_eq!(catalog.pinned(&["steam".to_owned()]), [AppId::new("steam")]);
        assert_eq!(catalog.command(&AppId::new("steam")).unwrap(), ["steam"]);
        assert_eq!(
            catalog.identify("steam.exe"),
            AppId::new("wine-Programs-Steam-Steam")
        );
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
    fn the_apps_one_can_start_are_listed_by_name_without_the_hidden_ones() {
        let (_dir, catalog) = catalog_of(&[
            ("zed.desktop", "Name=Zed\nExec=zed"),
            ("browser.desktop", "Name=browser\nExec=browser"),
            ("helper.desktop", "Name=Helper\nExec=helper\nNoDisplay=true"),
            ("removed.desktop", "Name=Removed\nExec=removed\nHidden=true"),
            ("broken.desktop", "Name=Broken"),
        ]);

        let listed: Vec<String> = catalog.launchable().into_iter().map(|(_, name)| name).collect();

        assert_eq!(listed, ["browser", "Zed"]);
    }

    fn eventually_exists(path: &std::path::Path) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        path.exists()
    }

    #[test]
    fn an_app_gets_a_scope_named_for_berth_and_the_app() {
        let scope = scope_for(&AppId::new("org.mozilla.firefox"));

        assert!(scope.starts_with("app-berth-org.mozilla.firefox-"), "{scope}");
        assert!(scope.ends_with(".scope"), "{scope}");
    }

    #[test]
    fn dashes_and_odd_characters_in_an_app_id_are_escaped_the_way_systemd_does() {
        assert!(scope_for(&AppId::new("my-app")).starts_with("app-berth-my\\x2dapp-"));
        assert!(scope_for(&AppId::new("a b/c")).starts_with("app-berth-a\\x20b-c-"));
        assert!(scope_for(&AppId::new(".hidden")).starts_with("app-berth-\\x2ehidden-"));
    }

    #[test]
    fn starting_an_app_twice_gives_each_start_its_own_scope() {
        let app = AppId::new("editor");

        assert_ne!(scope_for(&app), scope_for(&app));
    }

    #[test]
    fn under_a_user_manager_a_started_app_runs_inside_its_own_scope() {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_default();
        assert!(
            runtime.join("systemd/private").exists(),
            "this spec needs the systemd user manager of a real session"
        );
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("cgroup");
        let copy = format!(
            "cat /proc/self/cgroup > {}.part && mv {0}.part {0}",
            marker.display()
        );

        launch(&AppId::new("berth-spec"), &["sh".into(), "-c".into(), copy]).unwrap();

        assert!(eventually_exists(&marker));
        let cgroup = fs::read_to_string(&marker).unwrap();
        assert!(
            cgroup.contains("/app.slice/app-berth-berth\\x2dspec-"),
            "{cgroup}"
        );
        assert!(cgroup.trim_end().ends_with(".scope"), "{cgroup}");
    }

    #[test]
    fn without_a_user_manager_an_app_still_starts() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("started");

        launch_with(
            dir.path(),
            &AppId::new("touch"),
            &["touch".into(), marker.to_string_lossy().into_owned()],
        )
        .unwrap();

        assert!(eventually_exists(&marker));
    }

    #[test]
    fn launching_starts_the_program_with_its_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("started");

        launch(
            &AppId::new("touch"),
            &["touch".into(), marker.to_string_lossy().into_owned()],
        )
        .unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while !marker.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists());
    }

    #[test]
    fn a_program_that_cannot_be_started_is_reported() {
        let nowhere = tempfile::tempdir().unwrap();
        let app = AppId::new("missing");
        assert!(launch_with(nowhere.path(), &app, &["/nonexistent/berth-test-program".into()]).is_err());
        assert!(launch(&app, &[]).is_err());
    }
}
