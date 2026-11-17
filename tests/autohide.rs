mod support;

use std::time::Duration;

use support::*;

const GREY: [u8; 3] = [64, 64, 64];

fn dock_is_visible(desktop: &Desktop) -> bool {
    desktop
        .screenshot()
        .shows(GREEN, icon(1).center_x(), icon(1).center_y())
}

#[test]
fn the_dock_stays_while_windows_leave_its_space_alone() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));

    desktop.open("red", "small window", "404040", "400x300");

    never(
        "the dock must not hide from a window that does not touch it",
        Duration::from_secs(1),
        || !dock_is_visible(&desktop),
    );
}

#[test]
fn the_dock_makes_way_for_a_window_that_needs_its_space_and_returns_at_the_screen_edge() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    eventually("the dock is drawn", || dock_is_visible(&desktop));

    desktop.open("red", "big window", "404040", "maximized");
    eventually("the window has the bottom of the screen to itself", || {
        let shot = desktop.screenshot();
        shot.shows(GREY, icon(1).center_x(), icon(1).center_y()) && shot.count(GREEN) == 0
    });

    desktop.pointer(panel().center_x(), SCREEN.1 as f32 - 1.0);
    eventually("the dock slides back in under the pointer", || {
        dock_is_visible(&desktop)
    });

    desktop.pointer(400.0, 300.0);
    eventually("the dock hides again once the pointer has left", || {
        !dock_is_visible(&desktop)
    });
}

#[test]
fn the_dock_comes_back_when_the_window_in_its_way_is_gone() {
    let mut desktop = Desktop::with_dock(&format!(
        "[dock]\n{PINNED}\n[dock.actions]\nmiddle-click = \"close\"\n"
    ));
    desktop.open("green", "big window", "404040", "maximized");
    eventually("the dock is hidden", || desktop.screenshot().count(GREEN) == 0);

    desktop.pointer(panel().center_x(), SCREEN.1 as f32 - 1.0);
    eventually("the dock is back under the pointer", || dock_is_visible(&desktop));
    desktop.click_at(icon(1).center_x(), icon(1).center_y(), Button::Middle);
    eventually("the window is closed", || desktop.windows().is_empty());
    desktop.pointer(400.0, 300.0);

    never(
        "the dock must stay now that nothing is in its way",
        Duration::from_secs(1),
        || !dock_is_visible(&desktop),
    );
}

#[test]
fn a_dock_that_never_hides_keeps_maximized_windows_above_it() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\nautohide = \"never\"\n{PINNED}"));

    desktop.open("red", "big window", "404040", "maximized");

    eventually("the window fills the screen down to the dock", || {
        let shot = desktop.screenshot();
        shot.shows(GREY, 200.0, panel().y - 40.0) && !shot.shows(GREY, 200.0, panel().center_y())
    });
    never("the dock must stay visible", Duration::from_secs(1), || {
        !dock_is_visible(&desktop)
    });
}

#[test]
fn a_dock_that_always_hides_only_appears_at_the_screen_edge() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\nautohide = \"always\"\n{PINNED}"));
    eventually("the dock hides although the desktop is empty", || {
        desktop.screenshot().count(GREEN) == 0
    });

    desktop.pointer(panel().center_x(), SCREEN.1 as f32 - 1.0);

    eventually("the dock appears under the pointer", || dock_is_visible(&desktop));
}

fn stripes_seen_through_the_panel(look: &str) -> u32 {
    let mut desktop = Desktop::with_dock(&format!("[dock]\nmagnification = 1.0\n{look}{PINNED}"));
    desktop.open("red", "striped window", "ffffff+000000", "maximized");
    eventually("the dock is hidden", || desktop.screenshot().count(GREEN) == 0);
    desktop.pointer(panel().center_x(), SCREEN.1 as f32 - 1.0);
    eventually("the dock is back under the pointer", || dock_is_visible(&desktop));
    std::thread::sleep(Duration::from_millis(500));

    let shot = desktop.screenshot();
    let above_the_icons = panel().y + 4.0;
    let along_the_panel: Vec<u32> = (16..panel().width as u32 - 16)
        .map(|step| shot.brightness(panel().x + step as f32, above_the_icons))
        .collect();
    along_the_panel
        .windows(2)
        .map(|pair| pair[0].abs_diff(pair[1]))
        .max()
        .unwrap()
}

#[test]
fn what_lies_behind_the_dock_is_blurred_unless_blur_is_turned_off() {
    let blurred = stripes_seen_through_the_panel("");
    let sharp = stripes_seen_through_the_panel("blur = false\n");

    assert!(
        sharp > 150,
        "without blur the stripes keep their hard edges: {sharp}"
    );
    assert!(
        blurred < sharp / 3,
        "with blur they melt together: {blurred} against {sharp}"
    );
}

#[test]
fn a_solid_dock_hides_what_lies_behind_it() {
    let seen = stripes_seen_through_the_panel("opacity = 1.0\nblur = false\n");

    assert_eq!(seen, 0);
}
