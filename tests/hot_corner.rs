mod support;

use std::time::Duration;

use berth::geometry::Rect;
use berth::overview::layout::{Places, TITLE, arrange};
use support::*;

const AWAY: (f32, f32) = (700.0, 15.0);

fn three_windows(overview: &str) -> Desktop {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}{overview}"));
    desktop.open("red", "red window", "ff0000", "500x300");
    desktop.open("green", "green window", "00ff00", "500x300");
    desktop.open("blue", "blue window", "0000ff", "500x300");
    desktop.pointer(AWAY.0, AWAY.1);
    desktop
}

fn red_card() -> Rect {
    arrange((SCREEN.0 as f32, SCREEN.1 as f32), 3, 0).windows[2]
}

fn spread_out(desktop: &Desktop) -> bool {
    let cell = red_card();
    desktop
        .screenshot()
        .shows(RED, cell.center_x(), cell.y + (cell.height - TITLE) / 2.0)
}

fn poke(desktop: &mut Desktop, x: f32, y: f32) {
    desktop.pointer(AWAY.0, AWAY.1);
    desktop.pointer(x, y);
}

#[test]
fn pushing_the_pointer_into_the_top_left_corner_spreads_out_every_window() {
    let mut desktop = three_windows("");

    poke(&mut desktop, 0.0, 0.0);

    eventually("the overview is open", || spread_out(&desktop));
    assert_eq!(desktop.focused().as_deref(), Some("blue window"));
}

#[test]
fn poking_the_corner_again_puts_the_overview_away_and_pushing_on_into_it_brings_nothing_back() {
    let mut desktop = three_windows("");
    poke(&mut desktop, 0.0, 0.0);
    eventually("the overview is open", || spread_out(&desktop));

    poke(&mut desktop, 0.0, 0.0);
    eventually("the overview is gone", || !spread_out(&desktop));
    desktop.pointer(0.0, 0.0);
    desktop.pointer(0.0, 0.0);

    never(
        "the overview must not come back while the pointer stays in the corner",
        Duration::from_millis(800),
        || spread_out(&desktop),
    );
    assert_eq!(desktop.focused().as_deref(), Some("blue window"));
}

#[test]
fn after_escape_the_corner_opens_the_overview_again_on_the_next_poke() {
    let mut desktop = three_windows("");
    poke(&mut desktop, 0.0, 0.0);
    eventually("the overview is open", || spread_out(&desktop));
    desktop.tap(Key::Escape);
    eventually("escape closed it", || !spread_out(&desktop));

    poke(&mut desktop, 0.0, 0.0);

    eventually("the overview is open again", || spread_out(&desktop));
}

#[test]
fn the_hot_corner_can_be_the_top_right_one_instead() {
    let mut desktop = three_windows("[overview]\nhot-corner = \"top-right\"\n");

    poke(&mut desktop, 0.0, 0.0);
    never(
        "the top-left corner is not hot",
        Duration::from_millis(800),
        || spread_out(&desktop),
    );

    poke(&mut desktop, SCREEN.0 as f32 - 1.0, 0.0);
    eventually("the overview is open", || spread_out(&desktop));
}

#[test]
fn a_hot_corner_turned_off_opens_nothing() {
    let mut desktop = three_windows("[overview]\nhot-corner = \"off\"\n");

    poke(&mut desktop, 0.0, 0.0);

    never("nothing opens", Duration::from_millis(800), || {
        spread_out(&desktop)
    });
}

#[test]
fn only_a_corner_the_pointer_cannot_slip_past_is_hot_and_it_opens_the_overview_on_its_own_monitor() {
    const WIDE: (u32, u32) = (1920, 1080);
    const SMALL: (u32, u32) = (1280, 720);
    const LAYOUT: (u32, u32) = (WIDE.0 + SMALL.0, WIDE.1);
    let mut desktop = Desktop::on_sway_with_monitors(
        &[WIDE, SMALL],
        &format!("[dock]\n{PINNED}[overview]\nhot-corner = \"top-right\"\n"),
    );
    let search = |size: (u32, u32)| -> Places { arrange((size.0 as f32, size.1 as f32), 0, 0) };
    let searching = |desktop: &Desktop, monitor: &str, size| {
        desktop.monitor(monitor).brightest_in(search(size).search) > 300
    };

    desktop.pointer_across(900.0, 400.0, LAYOUT);
    desktop.pointer_across(WIDE.0 as f32 - 1.0, 0.0, LAYOUT);
    never(
        "the corner shared with the next monitor is not hot",
        Duration::from_millis(800),
        || searching(&desktop, "HEADLESS-1", WIDE) || searching(&desktop, "HEADLESS-2", SMALL),
    );

    desktop.pointer_across(2500.0, 400.0, LAYOUT);
    desktop.pointer_across(LAYOUT.0 as f32 - 1.0, 0.0, LAYOUT);
    eventually("the overview opens on the second monitor", || {
        searching(&desktop, "HEADLESS-2", SMALL)
    });
    assert!(!searching(&desktop, "HEADLESS-1", WIDE));
}
