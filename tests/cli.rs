mod support;

use std::fs;
use std::process::{Command, Output};

use support::*;

fn berth_alone(home: &tempfile::TempDir, arguments: &[&str]) -> Output {
    Command::new(berth())
        .args(arguments)
        .env("XDG_RUNTIME_DIR", home.path())
        .env("XDG_CONFIG_HOME", home.path())
        .env("WAYLAND_DISPLAY", "wayland-nowhere")
        .output()
        .unwrap()
}

fn complaint(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn asking_for_a_switch_while_nothing_runs_says_so() {
    let home = tempfile::tempdir().unwrap();

    let output = berth_alone(&home, &["switch", "next"]);

    assert!(!output.status.success());
    assert!(
        complaint(&output).starts_with("berth: berth is not running"),
        "{}",
        complaint(&output)
    );
}

#[test]
fn a_mistake_in_the_configuration_stops_startup_and_names_the_problem() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir(home.path().join("berth")).unwrap();
    fs::write(home.path().join("berth/config.toml"), "[dock]\nopacity = 7\n").unwrap();

    let output = berth_alone(&home, &[]);

    assert!(!output.status.success());
    assert_eq!(
        complaint(&output),
        "berth: dock.opacity must be between 0.0 and 1.0\n"
    );
}

#[test]
fn starting_without_a_compositor_says_so() {
    let home = tempfile::tempdir().unwrap();

    let output = berth_alone(&home, &[]);

    assert!(!output.status.success());
    assert!(
        complaint(&output).starts_with("berth: no Wayland compositor to connect to"),
        "{}",
        complaint(&output)
    );
}

#[test]
fn an_unknown_direction_is_rejected_with_the_ones_that_exist() {
    let home = tempfile::tempdir().unwrap();

    let output = berth_alone(&home, &["switch", "sideways"]);

    assert!(!output.status.success());
    assert!(
        complaint(&output).contains("next") && complaint(&output).contains("previous"),
        "{}",
        complaint(&output)
    );
}

#[test]
fn a_second_dock_refuses_to_start_beside_the_first() {
    let desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));

    let second = desktop.run(&[]);

    assert!(!second.status.success());
    assert!(
        complaint(&second).contains("already running"),
        "{}",
        complaint(&second)
    );
    assert!(desktop.run(&["windows"]).status.success());
}

#[test]
fn the_window_list_names_every_window_with_its_state() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    desktop.open("red", "red window", "ff0000", "500x300");
    desktop.open("green", "green window", "00ff00", "500x300");

    let listing = String::from_utf8(desktop.run(&["windows"]).stdout).unwrap();

    assert_eq!(listing, "focused\tgreen\tgreen window\nopen\tred\tred window\n");
}

#[test]
fn asked_to_stop_the_dock_leaves_the_screen_and_tidies_up_after_itself() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    eventually("the dock is drawn", || desktop.screenshot().count(GREEN) > 0);

    let farewell = desktop.ask_the_dock_to_stop();

    assert!(farewell.success(), "{farewell}");
    assert!(!desktop.socket().exists());
    eventually("the dock is off the screen", || {
        desktop.screenshot().count(GREEN) == 0
    });
    assert!(complaint(&desktop.run(&["windows"])).contains("not running"));

    desktop.start_dock(&format!("[dock]\n{PINNED}"));
    eventually("a new dock takes its place", || {
        desktop.screenshot().count(GREEN) > 0
    });
}
