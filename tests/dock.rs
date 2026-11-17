mod support;

use std::time::Duration;

use support::*;

#[test]
fn the_pinned_apps_wait_in_a_dock_at_the_bottom_of_the_screen() {
    let desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));

    eventually("the three pinned icons are on screen", || {
        let shot = desktop.screenshot();
        [RED, GREEN, BLUE]
            .iter()
            .enumerate()
            .all(|(index, colour)| shot.shows(*colour, icon(index).center_x(), icon(index).center_y()))
    });
    assert!(panel().bottom() <= SCREEN.1 as f32 && panel().bottom() > SCREEN.1 as f32 - 12.0);
}

#[test]
fn the_icon_under_the_pointer_swells_and_settles_back_when_the_pointer_leaves() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    let green = icon(1);
    let above_the_resting_icon = (green.center_x(), green.y - 14.0);
    eventually("the dock is drawn", || {
        desktop
            .screenshot()
            .shows(GREEN, green.center_x(), green.center_y())
    });
    assert!(
        !desktop
            .screenshot()
            .shows(GREEN, above_the_resting_icon.0, above_the_resting_icon.1)
    );

    desktop.pointer(green.center_x(), green.center_y());
    eventually("the green icon has grown upward", || {
        desktop
            .screenshot()
            .shows(GREEN, above_the_resting_icon.0, above_the_resting_icon.1)
    });

    desktop.pointer(400.0, 300.0);
    eventually("the green icon is back to its resting size", || {
        !desktop
            .screenshot()
            .shows(GREEN, above_the_resting_icon.0, above_the_resting_icon.1)
    });
}

#[test]
fn clicking_a_pinned_app_starts_it() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    assert_eq!(desktop.windows(), []);

    desktop.click_at(icon(0).center_x(), icon(0).center_y(), Button::Left);

    eventually("the red app has opened a window", || {
        desktop.focused().as_deref() == Some("red window")
    });
}

#[test]
fn an_app_started_from_the_dock_can_be_asked_to_quit_like_any_other() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    desktop.click_at(icon(0).center_x(), icon(0).center_y(), Button::Left);
    eventually("the red app has opened a window", || {
        desktop.focused().as_deref() == Some("red window")
    });

    desktop.ask_launched_app_to_quit("red");

    eventually("the red app is gone", || desktop.windows().is_empty());
}

#[test]
fn clicking_the_focused_app_minimizes_it_and_clicking_again_brings_it_back() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    desktop.open("red", "red window", "ff0000", "500x300");

    desktop.click_at(icon(0).center_x(), icon(0).center_y(), Button::Left);
    eventually("the red window is minimized", || {
        desktop.state_of("red window").as_deref() == Some("minimized")
    });

    desktop.click(Button::Left);
    eventually("the red window is focused again", || {
        desktop.focused().as_deref() == Some("red window")
    });
    assert_eq!(desktop.windows().len(), 1);
}

#[test]
fn shift_and_control_turn_a_click_into_different_gestures() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    desktop.open("red", "red window", "ff0000", "500x300");

    desktop.press(Key::Shift);
    desktop.click_at(icon(0).center_x(), icon(0).center_y(), Button::Left);
    desktop.release(Key::Shift);
    eventually("shift-click started a second red window", || {
        desktop.windows().len() == 2
    });

    desktop.press(Key::Control);
    desktop.click(Button::Left);
    desktop.release(Key::Control);
    eventually("control-click closed the one in front", || {
        desktop.windows().len() == 1
    });
}

#[test]
fn a_click_on_the_dock_leaves_the_keyboard_with_the_window_it_raised() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    desktop.open("red", "red window", "ff0000", "500x300");
    desktop.open("green", "green window", "00ff00", "500x300");

    desktop.click_at(icon(0).center_x(), icon(0).center_y(), Button::Left);
    eventually("the red window is raised", || {
        desktop.focused().as_deref() == Some("red window")
    });
    desktop.pointer(400.0, 300.0);

    never(
        "the red window must keep the focus",
        Duration::from_millis(800),
        || desktop.focused().as_deref() != Some("red window"),
    );
}

#[test]
fn scrolling_over_an_app_walks_through_its_windows() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));
    desktop.open("red", "first red", "ff0000", "500x300");
    desktop.open("red", "second red", "aa0000", "500x300");
    desktop.open("red", "third red", "770000", "500x300");

    desktop.pointer(icon(0).center_x(), icon(0).center_y());
    desktop.scroll(1);
    eventually("focus wrapped around to the first window", || {
        desktop.focused().as_deref() == Some("first red")
    });

    desktop.scroll(1);
    eventually("focus moved on to the second window", || {
        desktop.focused().as_deref() == Some("second red")
    });

    desktop.scroll(-1);
    eventually("scrolling back returned to the first window", || {
        desktop.focused().as_deref() == Some("first red")
    });
}

#[test]
fn every_gesture_does_what_the_configuration_says() {
    let mut desktop = Desktop::with_dock(&format!(
        "[dock]\n{PINNED}\n[dock.actions]\nclick = \"focus\"\nmiddle-click = \"close\"\nright-click = \"launch\"\n"
    ));
    desktop.open("red", "red window", "ff0000", "500x300");
    desktop.open("green", "green window", "00ff00", "500x300");

    desktop.click_at(icon(0).center_x(), icon(0).center_y(), Button::Left);
    eventually("a click only focuses", || {
        desktop.focused().as_deref() == Some("red window")
    });
    desktop.click(Button::Left);
    never(
        "a second click must not minimize",
        Duration::from_millis(600),
        || desktop.state_of("red window").as_deref() == Some("minimized"),
    );

    desktop.click(Button::Right);
    eventually("a right click starts another red window", || {
        desktop
            .windows()
            .iter()
            .filter(|window| window.app == "red")
            .count()
            == 2
    });

    desktop.click_at(icon(1).center_x(), icon(1).center_y(), Button::Middle);
    eventually("a middle click closes the green window", || {
        desktop.state_of("green window").is_none()
    });
}

#[test]
fn an_app_that_is_not_pinned_joins_the_dock_while_it_runs() {
    let mut desktop = Desktop::with_dock("[dock]\npinned = [\"red\"]\n");
    let alone = dock_of(1).1;
    eventually("only the red icon is shown", || {
        let shot = desktop.screenshot();
        shot.shows(RED, alone[0].center_x(), alone[0].center_y()) && shot.count(BLUE) == 0
    });

    desktop.open("blue", "blue window", "0000ff", "200x100");

    let pair = dock_of(2).1;
    eventually("the blue app has an icon next to the pinned one", || {
        let shot = desktop.screenshot();
        shot.shows(RED, pair[0].center_x(), pair[0].center_y())
            && shot.shows(BLUE, pair[1].center_x(), pair[1].center_y())
    });
}

#[test]
fn on_a_dense_display_the_dock_keeps_its_place_and_is_drawn_with_twice_the_detail() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}"));

    desktop.double_the_pixel_density();

    let (_, icons) = dock_on((SCREEN.0 / 2, SCREEN.1 / 2), 3);
    eventually("each icon fills twice as many pixels, edge to edge", || {
        let shot = desktop.screenshot();
        [RED, GREEN, BLUE].iter().zip(&icons).all(|(colour, icon)| {
            shot.shows(*colour, icon.x * 2.0 + 1.0, icon.y * 2.0 + 1.0)
                && shot.shows(*colour, icon.right() * 2.0 - 2.0, icon.bottom() * 2.0 - 2.0)
                && !shot.shows(*colour, icon.right() * 2.0 + 2.0, icon.center_y() * 2.0)
        })
    });
}
