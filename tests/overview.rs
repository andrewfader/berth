mod support;

use std::time::Duration;

use berth::geometry::Rect;
use berth::overview::layout::{Places, TITLE, arrange};
use support::*;

fn three_windows() -> Desktop {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    desktop.open("red", "red window", "ff0000", "500x300");
    desktop.open("green", "green window", "00ff00", "500x300");
    desktop.open("blue", "blue window", "0000ff", "500x300");
    desktop
}

fn places(windows: usize, apps: usize) -> Places {
    arrange((SCREEN.0 as f32, SCREEN.1 as f32), windows, apps)
}

fn picture_in(cell: Rect) -> (f32, f32) {
    (cell.center_x(), cell.y + (cell.height - TITLE) / 2.0)
}

fn shows_in(desktop: &Desktop, cell: Rect, colour: [u8; 3]) -> bool {
    let (x, y) = picture_in(cell);
    desktop.screenshot().shows(colour, x, y)
}

fn open_with_the_super_key(desktop: &mut Desktop) {
    desktop.tap(Key::Super);
    let cells = places(3, 0).windows;
    eventually("every window is laid out, most recent first", || {
        shows_in(desktop, cells[0], BLUE)
            && shows_in(desktop, cells[1], GREEN)
            && shows_in(desktop, cells[2], RED)
    });
}

#[test]
fn a_tap_on_the_super_key_spreads_out_every_window_over_a_dimmed_desktop() {
    let mut desktop = three_windows();
    let corner_before = desktop.screenshot().brightness(30.0, 400.0);

    open_with_the_super_key(&mut desktop);

    assert!(desktop.screenshot().brightness(30.0, 400.0) < corner_before);
    assert_eq!(desktop.focused().as_deref(), Some("blue window"));
}

#[test]
fn clicking_a_window_goes_to_it() {
    let mut desktop = three_windows();
    open_with_the_super_key(&mut desktop);
    let (x, y) = picture_in(places(3, 0).windows[2]);

    desktop.click_at(x, y, Button::Left);

    eventually("the red window has focus", || {
        desktop.focused().as_deref() == Some("red window")
    });
    eventually("the overview is gone", || {
        !shows_in(&desktop, places(3, 0).windows[1], GREEN)
    });
}

#[test]
fn the_arrow_keys_choose_a_window_and_enter_goes_to_it() {
    let mut desktop = three_windows();
    open_with_the_super_key(&mut desktop);

    desktop.tap(Key::Right);
    desktop.tap(Key::Right);
    desktop.tap(Key::Left);
    desktop.tap(Key::Enter);

    eventually("the second window, the green one, has focus", || {
        desktop.focused().as_deref() == Some("green window")
    });
}

#[test]
fn typing_narrows_the_windows_down_and_enter_goes_to_the_match() {
    let mut desktop = three_windows();
    open_with_the_super_key(&mut desktop);

    desktop.type_text("gre");
    eventually("only the green window is left, large in the middle", || {
        shows_in(&desktop, places(1, 1).windows[0], GREEN) && desktop.screenshot().count(RED) < 3000
    });

    desktop.tap(Key::Enter);
    eventually("the green window has focus", || {
        desktop.focused().as_deref() == Some("green window")
    });
}

#[test]
fn typing_the_name_of_an_app_offers_it_and_enter_starts_it() {
    let mut desktop = three_windows();
    open_with_the_super_key(&mut desktop);

    desktop.type_text("last");
    let tile = places(0, 1).apps[0];
    eventually("the app's icon is offered in the middle of the screen", || {
        desktop
            .screenshot()
            .shows(MAGENTA, tile.center_x(), tile.y + 48.0)
    });

    desktop.tap(Key::Enter);
    eventually("the app has started", || {
        desktop.focused().as_deref() == Some("last window")
    });
}

#[test]
fn escape_or_a_second_tap_on_super_puts_the_overview_away() {
    let mut desktop = three_windows();
    let cells = places(3, 0).windows;

    open_with_the_super_key(&mut desktop);
    desktop.tap(Key::Escape);
    eventually("escape closed it", || !shows_in(&desktop, cells[2], RED));

    open_with_the_super_key(&mut desktop);
    desktop.tap(Key::Super);
    eventually("a second tap closed it", || !shows_in(&desktop, cells[2], RED));
    assert_eq!(desktop.focused().as_deref(), Some("blue window"));
}

#[test]
fn a_click_on_the_empty_backdrop_puts_the_overview_away() {
    let mut desktop = three_windows();
    open_with_the_super_key(&mut desktop);

    desktop.click_at(20.0, 600.0, Button::Left);

    eventually("the overview is gone", || {
        !shows_in(&desktop, places(3, 0).windows[2], RED)
    });
    assert_eq!(desktop.focused().as_deref(), Some("blue window"));
}

#[test]
fn super_held_with_another_key_is_still_a_shortcut_and_opens_nothing() {
    let mut desktop = three_windows();
    let corner_before = desktop.screenshot().brightness(30.0, 400.0);

    desktop.press(Key::Super);
    desktop.tap(Key::Two);
    desktop.release(Key::Super);

    eventually("the shortcut switched to the empty second workspace", || {
        desktop.focused().is_none()
    });
    never("the overview must not open", Duration::from_millis(800), || {
        desktop.screenshot().brightness(30.0, 400.0) < corner_before
    });
}

#[test]
fn control_and_a_number_go_straight_to_that_window() {
    let mut desktop = three_windows();
    open_with_the_super_key(&mut desktop);

    desktop.press(Key::Control);
    desktop.tap(Key::Two);
    desktop.release(Key::Control);

    eventually("the second window shown, the green one, has focus", || {
        desktop.focused().as_deref() == Some("green window")
    });
    eventually("the overview is gone", || {
        !shows_in(&desktop, places(3, 0).windows[2], RED)
    });
}
