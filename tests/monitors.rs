mod support;

use berth::dock::layout::Metrics;
use support::*;

const WIDE: (u32, u32) = (1920, 1080);
const SMALL: (u32, u32) = (1280, 720);

fn has_a_dock(desktop: &Desktop, monitor: &str, size: (u32, u32)) -> bool {
    let (_, icons) = dock_on(size, 3);
    let shot = desktop.monitor(monitor);
    [RED, GREEN, BLUE]
        .iter()
        .zip(&icons)
        .all(|(colour, icon)| shot.shows(*colour, icon.center_x(), icon.center_y()))
}

#[test]
fn every_monitor_gets_its_own_dock() {
    let desktop = Desktop::on_sway_with_monitors(&[WIDE, SMALL], &format!("[dock]\n{PINNED}"));

    eventually("the first monitor has a dock", || {
        has_a_dock(&desktop, "HEADLESS-1", WIDE)
    });
    eventually("the second monitor has a dock", || {
        has_a_dock(&desktop, "HEADLESS-2", SMALL)
    });
}

#[test]
fn a_monitor_plugged_in_later_gets_a_dock_too() {
    let desktop = Desktop::on_sway_with_monitors(&[WIDE], &format!("[dock]\n{PINNED}"));
    eventually("the first monitor has a dock", || {
        has_a_dock(&desktop, "HEADLESS-1", WIDE)
    });

    desktop.sway(&["create_output"]);
    desktop.sway(&["output", "HEADLESS-2", "resolution", "1280x720", "scale", "1"]);

    eventually("the new monitor has a dock", || {
        has_a_dock(&desktop, "HEADLESS-2", SMALL)
    });
}

#[test]
fn unplugging_a_monitor_leaves_the_other_docks_running() {
    let desktop = Desktop::on_sway_with_monitors(&[WIDE, SMALL], &format!("[dock]\n{PINNED}"));
    eventually("the second monitor has a dock", || {
        has_a_dock(&desktop, "HEADLESS-2", SMALL)
    });

    desktop.sway(&["output", "HEADLESS-2", "unplug"]);

    eventually("berth still answers", || {
        desktop.run(&["windows"]).status.success()
    });
    eventually("the first monitor still has its dock", || {
        has_a_dock(&desktop, "HEADLESS-1", WIDE)
    });

    desktop.sway(&["create_output"]);
    desktop.sway(&["output", "HEADLESS-3", "resolution", "1280x720", "scale", "1"]);
    eventually("a replacement monitor gets a dock", || {
        has_a_dock(&desktop, "HEADLESS-3", SMALL)
    });
}

#[test]
fn a_crowded_dock_shrinks_its_icons_to_fit_a_narrow_monitor() {
    let everything = "pinned = [\"red\", \"extra1\", \"extra2\", \"extra3\", \"extra4\", \"extra5\", \"extra6\", \"extra7\", \"extra8\", \"extra9\", \"green\", \"blue\", \"last\"]\n";
    let desktop = Desktop::on_sway_with_monitors(&[(640, 480)], &format!("[dock]\n{everything}"));
    let fitted = Metrics::fitted(48.0, 1.6, 13, 640.0).icon;
    let a_whole_icon = ((fitted - 1.0) * (fitted - 1.0)) as usize;
    assert!(fitted < 48.0);

    eventually("the first and the last icon are both on screen in full", || {
        let shot = desktop.monitor("HEADLESS-1");
        shot.count(RED) >= a_whole_icon && shot.count(MAGENTA) >= a_whole_icon
    });
}
