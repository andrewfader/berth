mod support;

use std::time::Duration;

use support::*;

const SLOW_STARTER: &str = "[dock]\npinned = [\"red\", \"slow\"]\n";

fn hopping(desktop: &Desktop) -> bool {
    let slow = dock_of(2).1[1];
    desktop.screenshot().shows(MAGENTA, slow.center_x(), slow.y - 6.0)
}

fn start_the_slow_app(desktop: &mut Desktop) {
    let slow = dock_of(2).1[1];
    eventually("the dock is drawn", || desktop.screenshot().count(MAGENTA) > 0);
    desktop.click_at(slow.center_x(), slow.center_y(), Button::Left);
    desktop.pointer(400.0, 300.0);
    std::thread::sleep(Duration::from_millis(300));
}

#[test]
fn the_icon_of_a_starting_app_hops_until_its_window_appears() {
    let mut desktop = Desktop::with_dock(SLOW_STARTER);

    start_the_slow_app(&mut desktop);

    eventually("the icon is in the air", || hopping(&desktop));
    eventually("the app has opened its window", || {
        desktop.focused().as_deref() == Some("slow-window")
    });
    never(
        "the icon must rest once the app is up",
        Duration::from_secs(1),
        || hopping(&desktop),
    );
}

#[test]
fn the_hop_can_be_turned_off_on_its_own() {
    let mut desktop = Desktop::with_dock(&format!("{SLOW_STARTER}[animation]\nlaunch = \"none\"\n"));

    start_the_slow_app(&mut desktop);

    never("the icon must stay put", Duration::from_millis(1500), || {
        hopping(&desktop)
    });
    eventually("the app still starts", || {
        desktop.focused().as_deref() == Some("slow-window")
    });
}

#[test]
fn with_animations_off_the_dock_moves_in_a_single_step() {
    let mut desktop = Desktop::with_dock(&format!("[dock]\n{PINNED}[animation]\nenabled = false\n"));
    let green = icon(1);
    let grown = (green.center_x(), green.y - 14.0);
    eventually("the dock is drawn", || {
        desktop
            .screenshot()
            .shows(GREEN, green.center_x(), green.center_y())
    });

    desktop.pointer(green.center_x(), green.center_y());
    std::thread::sleep(Duration::from_millis(120));

    assert!(
        desktop.screenshot().shows(GREEN, grown.0, grown.1),
        "fully grown at once"
    );
    start_nothing_hops(&mut desktop);
}

fn start_nothing_hops(desktop: &mut Desktop) {
    desktop.click(Button::Left);
    desktop.pointer(400.0, 300.0);
    std::thread::sleep(Duration::from_millis(200));
    let green = icon(1);
    never("no hop with animations off", Duration::from_millis(600), || {
        desktop.screenshot().shows(GREEN, green.center_x(), green.y - 6.0)
    });
}
