mod support;

use std::time::Duration;

use support::*;

const EDITABLE: &str = "";

fn menu_is_showing(desktop: &Desktop, above: berth::geometry::Rect, rows: usize) -> bool {
    let whole = menu_area(above, rows);
    let clear_of_name_labels =
        berth::geometry::Rect::new(whole.x, whole.y, whole.width / 2.0 - 10.0, whole.height);
    desktop.screenshot().brightest_in(clear_of_name_labels) > 600
}

fn choose(desktop: &mut Desktop, above: berth::geometry::Rect, rows: usize, row: usize) {
    desktop.click_at(above.center_x(), above.center_y(), Button::Right);
    eventually("the menu is open", || menu_is_showing(desktop, above, rows));
    let (x, y) = menu_row(above, rows, row);
    desktop.click_at(x, y, Button::Left);
}

#[test]
fn a_right_click_offers_to_open_an_app_that_is_not_running() {
    let mut desktop = Desktop::with_dock(EDITABLE);
    eventually("the dock is drawn", || desktop.screenshot().count(RED) > 0);

    choose(&mut desktop, icon(0), 2, 0);

    eventually("the red app has opened a window", || {
        desktop.focused().as_deref() == Some("red window")
    });
    eventually("the menu is gone", || !menu_is_showing(&desktop, icon(0), 2));
}

#[test]
fn the_menu_lists_the_windows_of_a_running_app_and_raises_the_one_chosen() {
    let mut desktop = Desktop::with_dock(EDITABLE);
    desktop.open("red", "red window", "ff0000", "500x300");
    desktop.open("green", "green window", "00ff00", "500x300");

    choose(&mut desktop, icon(0), 4, 0);

    eventually("the red window has focus", || {
        desktop.focused().as_deref() == Some("red window")
    });
}

#[test]
fn new_window_starts_another_and_quit_closes_them_all() {
    let mut desktop = Desktop::with_dock(EDITABLE);
    desktop.open("red", "red window", "ff0000", "500x300");

    choose(&mut desktop, icon(0), 4, 1);
    eventually("a second red window opened", || desktop.windows().len() == 2);

    choose(&mut desktop, icon(0), 5, 4);
    eventually("every red window is closed", || desktop.windows().is_empty());
}

#[test]
fn an_app_unpinned_from_the_menu_stays_off_the_dock_after_a_restart() {
    let mut desktop = Desktop::with_dock(EDITABLE);
    eventually("the dock is drawn", || desktop.screenshot().count(GREEN) > 0);

    choose(&mut desktop, icon(1), 2, 1);

    let (_, two) = dock_of(2);
    let only_red_and_blue = |desktop: &Desktop| {
        let shot = desktop.screenshot();
        shot.count(GREEN) == 0
            && shot.shows(RED, two[0].center_x(), two[0].center_y())
            && shot.shows(BLUE, two[1].center_x(), two[1].center_y())
    };
    eventually("green has left the dock", || only_red_and_blue(&desktop));

    assert!(desktop.ask_the_dock_to_stop().success());
    desktop.start_dock(EDITABLE);
    eventually("green is still gone", || only_red_and_blue(&desktop));
}

#[test]
fn a_running_app_pinned_from_the_menu_keeps_its_place_once_it_has_quit() {
    let mut desktop = Desktop::with_dock(EDITABLE);
    desktop.open("last", "last window", "ff00ff", "300x200");
    let (_, four) = dock_of(4);
    eventually("the app has joined the dock", || {
        desktop
            .screenshot()
            .shows(MAGENTA, four[3].center_x(), four[3].center_y())
    });

    choose(&mut desktop, four[3], 4, 2);
    desktop.pointer(400.0, 900.0);
    desktop.kill_window("last window");
    eventually("its window is gone", || desktop.windows().is_empty());

    never("the pinned icon must stay", Duration::from_secs(1), || {
        !desktop
            .screenshot()
            .shows(MAGENTA, four[3].center_x(), four[3].center_y())
    });
}

#[test]
fn pinning_is_not_offered_when_the_configuration_file_lists_the_pins() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    eventually("the dock is drawn", || desktop.screenshot().count(RED) > 0);

    desktop.click_at(icon(0).center_x(), icon(0).center_y(), Button::Right);

    eventually("a one-line menu is open", || {
        menu_is_showing(&desktop, icon(0), 1)
    });
    let where_a_second_line_would_be = menu_area(icon(0), 2);
    let above_the_menu = berth::geometry::Rect::new(
        where_a_second_line_would_be.x,
        where_a_second_line_would_be.y,
        where_a_second_line_would_be.width,
        24.0,
    );
    assert!(desktop.screenshot().brightest_in(above_the_menu) < 400);
}

#[test]
fn the_menu_waits_while_the_pointer_wanders_and_closes_on_a_click_elsewhere() {
    let mut desktop = Desktop::with_dock(EDITABLE);
    eventually("the dock is drawn", || desktop.screenshot().count(RED) > 0);
    desktop.click_at(icon(0).center_x(), icon(0).center_y(), Button::Right);
    eventually("the menu is open", || menu_is_showing(&desktop, icon(0), 2));

    desktop.pointer(300.0, 300.0);
    never(
        "the menu must wait for a decision",
        Duration::from_millis(1200),
        || !menu_is_showing(&desktop, icon(0), 2),
    );

    desktop.click(Button::Left);
    eventually("the menu has closed", || !menu_is_showing(&desktop, icon(0), 2));
    assert_eq!(desktop.windows(), []);
}

#[test]
fn a_dock_hidden_under_a_window_stays_out_while_its_menu_is_open() {
    let mut desktop = Desktop::with_dock(EDITABLE);
    desktop.open("red", "big window", "404040", "maximized");
    eventually("the dock is hidden", || desktop.screenshot().count(GREEN) == 0);
    desktop.pointer(panel().center_x(), SCREEN.1 as f32 - 1.0);
    eventually("the dock is back under the pointer", || {
        desktop.screenshot().count(GREEN) > 0
    });

    desktop.click_at(icon(1).center_x(), icon(1).center_y(), Button::Right);
    eventually("the menu is open", || menu_is_showing(&desktop, icon(1), 2));
    let (x, y) = menu_row(icon(1), 2, 0);
    desktop.pointer(x, y);

    never(
        "the dock must not slide away under its menu",
        Duration::from_secs(1),
        || desktop.screenshot().count(GREEN) == 0,
    );
}

#[test]
fn the_menu_can_be_worked_from_the_keyboard() {
    let mut desktop = Desktop::with_dock(EDITABLE);
    desktop.open("red", "red window", "ff0000", "500x300");
    desktop.open("green", "green window", "00ff00", "500x300");
    desktop.click_at(icon(0).center_x(), icon(0).center_y(), Button::Right);
    eventually("the menu is open", || menu_is_showing(&desktop, icon(0), 4));
    desktop.pointer(icon(2).center_x(), icon(2).center_y());

    desktop.tap(Key::Down);
    desktop.tap(Key::Down);
    desktop.tap(Key::Up);
    desktop.tap(Key::Enter);

    eventually("enter chose the first entry, the red window", || {
        desktop.focused().as_deref() == Some("red window")
    });
    eventually("the menu is gone", || !menu_is_showing(&desktop, icon(0), 4));
}

#[test]
fn escape_closes_the_menu_and_changes_nothing() {
    let mut desktop = Desktop::with_dock(EDITABLE);
    eventually("the dock is drawn", || desktop.screenshot().count(RED) > 0);
    desktop.click_at(icon(0).center_x(), icon(0).center_y(), Button::Right);
    eventually("the menu is open", || menu_is_showing(&desktop, icon(0), 2));

    desktop.pointer(icon(2).center_x(), icon(2).center_y());
    desktop.tap(Key::Escape);

    eventually("the menu is gone", || !menu_is_showing(&desktop, icon(0), 2));
    assert_eq!(desktop.windows(), []);
}
