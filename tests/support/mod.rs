#![allow(dead_code)]

mod input;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, Once};
use std::thread::sleep;
use std::time::{Duration, Instant};

use berth::dock::layout::{Layout, Metrics, Pose};
use berth::dock::menu;
use berth::geometry::Rect;
use berth::switcher::carousel;
pub use input::{Button, Key};

pub const SCREEN: (u32, u32) = (1920, 1080);
pub const RED: [u8; 3] = [255, 0, 0];
pub const GREEN: [u8; 3] = [0, 255, 0];
pub const BLUE: [u8; 3] = [0, 0, 255];
pub const MAGENTA: [u8; 3] = [255, 0, 255];
pub const PINNED: &str = "pinned = [\"red\", \"green\", \"blue\"]\n";

const PATIENCE: Duration = Duration::from_secs(20);
const HOST_DISPLAY: &str = "wayland-1";
const DISPLAY: &str = "wayland-2";

static ONE_DESKTOP_AT_A_TIME: Mutex<()> = Mutex::new(());
static DESKTOPS: AtomicU32 = AtomicU32::new(0);

pub struct Desktop {
    root: PathBuf,
    session: Vec<Child>,
    opened: Vec<(String, u32)>,
    dock: Option<Child>,
    host: input::Host,
    display: &'static str,
    _turn: MutexGuard<'static, ()>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Listed {
    pub state: String,
    pub app: String,
    pub title: String,
}

pub struct Screenshot {
    width: u32,
    pixels: Vec<u8>,
}

impl Screenshot {
    pub fn at(&self, x: f32, y: f32) -> [u8; 3] {
        let offset = ((y as u32 * self.width + x as u32) * 3) as usize;
        [
            self.pixels[offset],
            self.pixels[offset + 1],
            self.pixels[offset + 2],
        ]
    }

    pub fn shows(&self, colour: [u8; 3], x: f32, y: f32) -> bool {
        self.at(x, y)
            .iter()
            .zip(colour)
            .all(|(seen, wanted)| seen.abs_diff(wanted) <= 12)
    }

    pub fn brightest_in(&self, area: Rect) -> u32 {
        let (left, top) = (area.x.max(0.0) as u32, area.y.max(0.0) as u32);
        (top..area.bottom() as u32)
            .flat_map(|y| (left..area.right() as u32).map(move |x| (x, y)))
            .map(|(x, y)| self.brightness(x as f32, y as f32))
            .max()
            .unwrap_or(0)
    }

    pub fn brightness(&self, x: f32, y: f32) -> u32 {
        self.at(x, y).iter().map(|channel| *channel as u32).sum()
    }

    pub fn count(&self, colour: [u8; 3]) -> usize {
        self.pixels
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|pixel| {
                pixel
                    .iter()
                    .zip(colour)
                    .all(|(seen, wanted)| seen.abs_diff(wanted) <= 12)
            })
            .count()
    }
}

pub fn berth() -> &'static str {
    env!("CARGO_BIN_EXE_berth")
}

fn window_program() -> PathBuf {
    static BUILT: Once = Once::new();
    BUILT.call_once(|| {
        let beside_the_dock = Path::new(berth()).ancestors().nth(2).expect("a target directory");
        let built = Command::new(env!("CARGO"))
            .args(["build", "--quiet", "--example", "window", "--target-dir"])
            .arg(beside_the_dock)
            .status();
        assert!(
            built.is_ok_and(|status| status.success()),
            "the test window example must build"
        );
    });
    Path::new(berth()).with_file_name("examples").join("window")
}

pub fn dock_on(screen: (u32, u32), items: usize) -> (Rect, Vec<Rect>) {
    let metrics = Metrics::for_icon(48.0, 1.6);
    let (wide, tall) = metrics.surface(items);
    let origin = ((screen.0 as f32 - wide) / 2.0, screen.1 as f32 - tall);
    let layout = Layout::arrange(&metrics, items, Pose::RESTING);
    let icons = layout
        .icons
        .iter()
        .map(|icon| icon.moved(origin.0, origin.1))
        .collect();
    (layout.panel.moved(origin.0, origin.1), icons)
}

pub fn dock_of(items: usize) -> (Rect, Vec<Rect>) {
    dock_on(SCREEN, items)
}

pub fn center_stage() -> Rect {
    let stage = carousel::stage_on(SCREEN);
    carousel::front_frame((stage.width, stage.height)).moved(stage.x, stage.y)
}

pub fn menu_row(above: Rect, rows: usize, row: usize) -> (f32, f32) {
    let (_, tall) = menu::size(rows, 0.0);
    let top = above.y - 8.0 - tall;
    (
        above.center_x(),
        top + menu::MARGIN + menu::ROW * (row as f32 + 0.5),
    )
}

pub fn menu_area(above: Rect, rows: usize) -> Rect {
    let (wide, tall) = menu::size(rows, 0.0);
    Rect::new(above.center_x() - wide / 2.0, above.y - 8.0 - tall, wide, tall)
}

pub fn panel() -> Rect {
    dock_of(3).0
}

pub fn icon(index: usize) -> Rect {
    dock_of(3).1[index]
}

pub fn eventually(what: &str, mut holds: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !holds() {
        assert!(Instant::now() < deadline, "gave up waiting until {what}");
        sleep(Duration::from_millis(40));
    }
}

pub fn never(what: &str, within: Duration, mut happens: impl FnMut() -> bool) {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        assert!(!happens(), "{what}");
        sleep(Duration::from_millis(40));
    }
}

impl Desktop {
    pub fn boot() -> Self {
        let mut desktop = Self::hosted();
        let compositor = desktop
            .command("dbus-run-session")
            .args(["--", "cosmic-comp", "--no-xwayland"])
            .env("WAYLAND_DISPLAY", HOST_DISPLAY)
            .env("COSMIC_BACKEND", "winit")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .spawn()
            .expect("cosmic-comp and dbus-run-session are needed for the test desktop");
        desktop.session.push(compositor);
        desktop.await_socket(DISPLAY);
        desktop.display = DISPLAY;
        desktop.await_first_frame();
        desktop
    }

    pub fn on_sway_with_monitors(sizes: &[(u32, u32)], config: &str) -> Self {
        let mut desktop = Self::hosted();
        for (index, (wide, tall)) in sizes.iter().enumerate() {
            let name = format!("HEADLESS-{}", index + 1);
            if index > 0 {
                desktop.sway(&["create_output"]);
            }
            desktop.sway(&[
                "output",
                &name,
                "resolution",
                &format!("{wide}x{tall}"),
                "scale",
                "1",
            ]);
        }
        desktop.await_first_frame();
        desktop.start_dock(config);
        desktop
    }

    pub fn sway(&self, command: &[&str]) {
        let socket = fs::read_dir(self.root.join("rt"))
            .unwrap()
            .filter_map(|entry| Some(entry.ok()?.path()))
            .find(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("sway-ipc"))
            })
            .expect("sway's control socket");
        let obeyed = self
            .command("swaymsg")
            .arg("-s")
            .arg(socket)
            .args(command)
            .status()
            .unwrap();
        assert!(obeyed.success(), "sway refused {command:?}");
    }

    pub fn monitor(&self, name: &str) -> Screenshot {
        let mut seen = Err(String::new());
        let deadline = Instant::now() + PATIENCE;
        while seen.is_err() && Instant::now() < deadline {
            seen = self.capture(&["-o", name]);
        }
        seen.unwrap_or_else(|problem| panic!("{problem}"))
    }

    fn await_first_frame(&self) {
        let mut seen = Err(String::new());
        let deadline = Instant::now() + PATIENCE;
        while seen.is_err() && Instant::now() < deadline {
            sleep(Duration::from_millis(50));
            seen = self.try_screenshot();
        }
        if let Err(problem) = seen {
            panic!("the test desktop never rendered: {problem}");
        }
    }

    fn hosted() -> Self {
        let turn = ONE_DESKTOP_AT_A_TIME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "d{}_{}",
            std::process::id(),
            DESKTOPS.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        for dir in [
            "rt",
            "cfg/berth",
            "cfg/cosmic/com.system76.CosmicSettings.Shortcuts/v1",
            "cfg/cosmic/com.system76.CosmicAppList/v1",
            "data/applications",
            "state",
            "cache",
            "shim",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::set_permissions(root.join("rt"), fs::Permissions::from_mode(0o700)).unwrap();
        for stub in ["systemctl", "dbus-update-activation-environment"] {
            fs::write(root.join("shim").join(stub), "#!/bin/sh\nexit 0\n").unwrap();
            fs::set_permissions(root.join("shim").join(stub), fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::write(
            root.join("sway.conf"),
            format!(
                "output HEADLESS-1 resolution {}x{} scale 1\ndefault_border none\n",
                SCREEN.0, SCREEN.1
            ),
        )
        .unwrap();
        fs::write(
            root.join("cfg/cosmic/com.system76.CosmicSettings.Shortcuts/v1/system_actions"),
            format!(
                "{{\n    WindowSwitcher: \"{0} switch next\",\n    WindowSwitcherPrevious: \"{0} switch previous\",\n}}\n",
                berth()
            ),
        )
        .unwrap();
        fs::write(
            root.join("cfg/cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom"),
            format!(
                "{{\n    (modifiers: [Super]): Spawn(\"{0} overview\"),\n    (modifiers: [Super, Ctrl], key: \"1\"): Spawn(\"{0} go 1\"),\n    (modifiers: [Super, Ctrl], key: \"2\"): Spawn(\"{0} go 2\"),\n}}\n",
                berth()
            ),
        )
        .unwrap();
        fs::write(
            root.join("cfg/cosmic/com.system76.CosmicAppList/v1/favorites"),
            "[\"red\", \"green\", \"blue\"]",
        )
        .unwrap();
        let mut apps = vec![
            ("red".to_owned(), "ff0000"),
            ("green".to_owned(), "00ff00"),
            ("blue".to_owned(), "0000ff"),
        ];
        apps.extend((1..=9).map(|number| (format!("extra{number}"), "808080")));
        apps.push(("last".to_owned(), "ff00ff"));
        fs::write(
            root.join("data/applications/slow.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=slow app\nIcon={}\nExec=sh -c \"sleep 2; exec {} slow slow-window ffff00 500x300\"\n",
                root.join("data/last.svg").display(),
                window_program().display()
            ),
        )
        .unwrap();
        for (name, colour) in apps.iter().map(|(name, colour)| (name.as_str(), *colour)) {
            let icon = root.join(format!("data/{name}.svg"));
            fs::write(
                &icon,
                format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="#{colour}"/></svg>"##),
            )
            .unwrap();
            fs::write(
                root.join(format!("data/applications/{name}.desktop")),
                format!(
                    "[Desktop Entry]\nType=Application\nName={name} app\nIcon={}\nExec={} {name} \"{name} window\" {colour} 500x300\n",
                    icon.display(),
                    window_program().display()
                ),
            )
            .unwrap();
        }

        let mut desktop = Self {
            session: Vec::new(),
            opened: Vec::new(),
            dock: None,
            host: input::Host::unconnected(),
            display: HOST_DISPLAY,
            root,
            _turn: turn,
        };
        let sway = desktop
            .command("sway")
            .arg("-c")
            .arg(desktop.root.join("sway.conf"))
            .env("WLR_BACKENDS", "headless")
            .env("WLR_LIBINPUT_NO_DEVICES", "1")
            .spawn()
            .expect("sway is needed to host the test desktop");
        desktop.session.push(sway);
        desktop.await_socket(HOST_DISPLAY);

        desktop.host = input::Host::connect(&desktop.root.join("rt").join(HOST_DISPLAY), SCREEN);
        desktop
    }

    pub fn with_dock(config: &str) -> Self {
        let mut desktop = Self::boot();
        desktop.start_dock(config);
        desktop
    }

    pub fn configure(&self, config: &str) {
        fs::write(self.root.join("cfg/berth/config.toml"), config).unwrap();
    }

    pub fn start_dock(&mut self, config: &str) {
        self.configure(config);
        let without_a_dock = self.screenshot().pixels;
        self.dock = Some(self.app(berth()).spawn().unwrap());
        eventually("the dock answers", || self.run(&["windows"]).status.success());
        eventually("the dock is on screen", || {
            self.screenshot().pixels != without_a_dock
        });
    }

    pub fn open(&mut self, app: &str, title: &str, colour: &str, placement: &str) {
        let window = self
            .app(window_program())
            .args([app, title, colour, placement])
            .spawn()
            .unwrap();
        self.opened.push((title.to_owned(), window.id()));
        self.session.push(window);
        eventually(&format!("{title} has focus"), || {
            self.windows()
                .first()
                .is_some_and(|window| window.title == title && window.state == "focused")
        });
    }

    pub fn double_the_pixel_density(&mut self) {
        let rescaled = self
            .command("cosmic-randr")
            .env("WAYLAND_DISPLAY", self.display)
            .args([
                "mode",
                "WINIT-0",
                &SCREEN.0.to_string(),
                &SCREEN.1.to_string(),
                "--scale",
                "2",
            ])
            .status()
            .expect("cosmic-randr is needed to change the test display");
        assert!(rescaled.success());
    }

    pub fn ask_the_dock_to_stop(&mut self) -> ExitStatus {
        let mut dock = self.dock.take().expect("a running dock");
        Command::new("kill")
            .arg("-TERM")
            .arg(dock.id().to_string())
            .status()
            .unwrap();
        dock.wait().unwrap()
    }

    pub fn socket(&self) -> PathBuf {
        self.root.join("rt").join(format!("berth-{}.sock", self.display))
    }

    pub fn ask_launched_app_to_quit(&self, app: &str) {
        let started_by_the_dock = format!("{} {app} ", window_program().display());
        Command::new("pkill")
            .args(["-TERM", "-f", &started_by_the_dock])
            .status()
            .unwrap();
    }

    pub fn kill_window(&mut self, title: &str) {
        let (_, process) = self
            .opened
            .iter()
            .find(|(opened, _)| opened == title)
            .expect("a window opened by this test");
        Command::new("kill").arg(process.to_string()).status().unwrap();
    }

    pub fn run(&self, arguments: &[&str]) -> Output {
        self.app(berth())
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap()
    }

    pub fn windows(&self) -> Vec<Listed> {
        String::from_utf8_lossy(&self.run(&["windows"]).stdout)
            .lines()
            .filter_map(|line| {
                let mut fields = line.splitn(3, '\t').map(str::to_owned);
                Some(Listed {
                    state: fields.next()?,
                    app: fields.next()?,
                    title: fields.next()?,
                })
            })
            .collect()
    }

    pub fn focused(&self) -> Option<String> {
        self.windows()
            .into_iter()
            .find(|window| window.state == "focused")
            .map(|window| window.title)
    }

    pub fn titles(&self) -> Vec<String> {
        self.windows().into_iter().map(|window| window.title).collect()
    }

    pub fn state_of(&self, title: &str) -> Option<String> {
        self.windows()
            .into_iter()
            .find(|window| window.title == title)
            .map(|window| window.state)
    }

    pub fn screenshot(&self) -> Screenshot {
        self.try_screenshot()
            .unwrap_or_else(|problem| panic!("{problem}"))
    }

    pub fn pointer(&mut self, x: f32, y: f32) {
        self.host.pointer(x, y);
    }

    pub fn pointer_across(&mut self, x: f32, y: f32, layout: (u32, u32)) {
        self.host.pointer_across(x, y, layout);
    }

    pub fn click(&mut self, button: Button) {
        self.host.click(button);
    }

    pub fn click_at(&mut self, x: f32, y: f32, button: Button) {
        self.host.pointer(x, y);
        sleep(Duration::from_millis(120));
        self.host.click(button);
    }

    pub fn scroll(&mut self, notches: i32) {
        self.host.scroll(notches);
    }

    pub fn press(&mut self, key: Key) {
        self.host.key(key, true);
    }

    pub fn release(&mut self, key: Key) {
        self.host.key(key, false);
    }

    pub fn type_text(&mut self, text: &str) {
        for letter in text.chars() {
            self.host.letter(letter);
        }
    }

    pub fn tap(&mut self, key: Key) {
        self.host.key(key, true);
        self.host.key(key, false);
    }

    fn try_screenshot(&self) -> Result<Screenshot, String> {
        self.capture(&[])
    }

    fn capture(&self, of: &[&str]) -> Result<Screenshot, String> {
        let output = self
            .command("grim")
            .env("WAYLAND_DISPLAY", self.display)
            .args(of)
            .args(["-t", "ppm", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("grim is needed to look at the test desktop");
        let complaint = || format!("grim: {}", String::from_utf8_lossy(&output.stderr));
        let mut fields = output.stdout.splitn(4, |byte| byte.is_ascii_whitespace());
        let (Some(b"P6"), Some(width), Some(_), Some(rest)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(complaint());
        };
        let width = std::str::from_utf8(width)
            .ok()
            .and_then(|width| width.parse().ok())
            .ok_or_else(complaint)?;
        let pixels = rest
            .splitn(2, |byte| byte.is_ascii_whitespace())
            .nth(1)
            .ok_or_else(complaint)?
            .to_vec();
        Ok(Screenshot { width, pixels })
    }

    fn app(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = self.command(program);
        command
            .env("WAYLAND_DISPLAY", self.display)
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_DATA_DIRS", self.root.join("data"));
        command
    }

    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let path = std::env::var("PATH").unwrap_or_default();
        let mut command = Command::new(program);
        command
            .env("XDG_RUNTIME_DIR", self.root.join("rt"))
            .env("XDG_CONFIG_HOME", self.root.join("cfg"))
            .env("XDG_STATE_HOME", self.root.join("state"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("PATH", format!("{}:{path}", self.root.join("shim").display()))
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}", self.root.join("rt/no-bus").display()),
            )
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .env_remove("NOTIFY_SOCKET")
            .env_remove("COSMIC_SESSION_SOCK")
            .env_remove("SWAYSOCK")
            .env_remove("I3SOCK")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .process_group(0);
        command
    }

    fn await_socket(&self, display: &str) {
        let socket = self.root.join("rt").join(display);
        eventually(&format!("{display} is up"), || socket.exists());
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        self.session.extend(self.dock.take());
        for process in self.session.iter_mut().rev() {
            let _ = Command::new("kill")
                .arg("-TERM")
                .arg(format!("-{}", process.id()))
                .status();
        }
        for process in self.session.iter_mut().rev() {
            let _ = process.wait();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
