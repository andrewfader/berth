mod support;

use std::time::Duration;

use support::*;

fn three_windows() -> Desktop {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    desktop.open("red", "red window", "ff0000", "500x300");
    desktop.open("green", "green window", "00ff00", "500x300");
    desktop.open("blue", "blue window", "0000ff", "500x300");
    desktop
}

fn on_stage(desktop: &Desktop, colour: [u8; 3]) -> bool {
    let front = center_stage();
    desktop
        .screenshot()
        .shows(colour, front.center_x(), front.center_y())
}

#[test]
fn a_quick_alt_tab_returns_to_the_window_used_before() {
    let mut desktop = three_windows();

    desktop.press(Key::Alt);
    desktop.tap(Key::Tab);
    desktop.release(Key::Alt);
    eventually("the green window has focus", || {
        desktop.focused().as_deref() == Some("green window")
    });

    desktop.press(Key::Alt);
    desktop.tap(Key::Tab);
    desktop.release(Key::Alt);
    eventually("the blue window has focus again", || {
        desktop.focused().as_deref() == Some("blue window")
    });
}

#[test]
fn holding_alt_presents_the_windows_and_each_tab_moves_on_to_the_next() {
    let mut desktop = three_windows();
    let corner_before = desktop.screenshot().brightness(40.0, 40.0);

    desktop.press(Key::Alt);
    desktop.tap(Key::Tab);
    eventually("the previous window is presented at center stage", || {
        on_stage(&desktop, GREEN)
    });
    assert!(
        desktop.screenshot().brightness(40.0, 40.0) < corner_before,
        "the desktop behind is dimmed"
    );
    assert_eq!(
        desktop.focused().as_deref(),
        Some("blue window"),
        "nothing switches while alt is held"
    );

    desktop.tap(Key::Tab);
    eventually("the selection moved on to the oldest window", || {
        on_stage(&desktop, RED)
    });

    desktop.release(Key::Alt);
    eventually("the red window has focus", || {
        desktop.focused().as_deref() == Some("red window")
    });
    eventually("the switcher is gone", || {
        desktop.screenshot().brightness(40.0, 40.0) == corner_before
    });
}

#[test]
fn shift_tab_walks_backwards() {
    let mut desktop = three_windows();

    desktop.press(Key::Alt);
    desktop.press(Key::Shift);
    desktop.tap(Key::Tab);
    desktop.release(Key::Shift);
    eventually("the least recently used window is presented", || {
        on_stage(&desktop, RED)
    });

    desktop.release(Key::Alt);
    eventually("the red window has focus", || {
        desktop.focused().as_deref() == Some("red window")
    });
}

#[test]
fn escape_leaves_everything_as_it_was() {
    let mut desktop = three_windows();
    desktop.press(Key::Alt);
    desktop.tap(Key::Tab);
    eventually("the switcher is showing", || on_stage(&desktop, GREEN));

    desktop.tap(Key::Escape);
    eventually("the switcher is gone", || !on_stage(&desktop, GREEN));
    desktop.release(Key::Alt);

    never("focus must not move", Duration::from_millis(700), || {
        desktop.focused().as_deref() != Some("blue window")
    });
}

#[test]
fn clicking_a_window_in_the_row_switches_to_it() {
    let mut desktop = three_windows();
    desktop.press(Key::Alt);
    desktop.tap(Key::Tab);
    eventually("the switcher is showing", || on_stage(&desktop, GREEN));
    let front = center_stage();

    desktop.click_at(front.x - 60.0, front.bottom() - 40.0, Button::Left);

    eventually("the window to the left of the selection has focus", || {
        desktop.focused().as_deref() == Some("blue window")
    });
    desktop.release(Key::Alt);
    eventually("the switcher is gone", || !on_stage(&desktop, GREEN));
}

#[test]
fn a_window_closed_while_the_switcher_is_open_drops_out_of_the_row() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    desktop.open("red", "red window", "ff0000", "500x300");
    desktop.open("green", "green window", "00ff00", "500x300");
    desktop.press(Key::Alt);
    desktop.tap(Key::Tab);
    eventually("the red window is presented", || on_stage(&desktop, RED));

    desktop.kill_window("red window");

    eventually("the remaining window takes center stage", || {
        on_stage(&desktop, GREEN)
    });
    desktop.release(Key::Alt);
    eventually("the green window has focus", || {
        desktop.focused().as_deref() == Some("green window")
    });
}

#[test]
fn asked_from_the_command_line_with_no_key_held_it_switches_straight_away() {
    let desktop = three_windows();

    let switched = desktop.run(&["switch", "next"]);

    assert!(switched.status.success());
    eventually("the green window has focus", || {
        desktop.focused().as_deref() == Some("green window")
    });
}

#[test]
fn there_is_nothing_to_show_on_an_empty_desktop() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    let corner_before = desktop.screenshot().brightness(40.0, 40.0);

    desktop.press(Key::Alt);
    desktop.tap(Key::Tab);

    never("the desktop must not dim", Duration::from_millis(700), || {
        desktop.screenshot().brightness(40.0, 40.0) != corner_before
    });
    desktop.release(Key::Alt);
}

#[test]
fn on_a_dense_display_the_switcher_fills_the_same_part_of_the_screen() {
    let mut desktop = three_windows();
    desktop.double_the_pixel_density();
    let stage = berth::switcher::carousel::stage_on((SCREEN.0 / 2, SCREEN.1 / 2));
    let front = berth::switcher::carousel::front_frame((stage.width, stage.height)).moved(stage.x, stage.y);

    desktop.press(Key::Alt);
    desktop.tap(Key::Tab);

    eventually("the previous window is presented at center stage", || {
        desktop
            .screenshot()
            .shows(GREEN, front.center_x() * 2.0, front.center_y() * 2.0)
    });
    desktop.release(Key::Alt);
    eventually("the green window has focus", || {
        desktop.focused().as_deref() == Some("green window")
    });
}
