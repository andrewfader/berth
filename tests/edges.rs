mod support;

use berth::dock::layout::{Edge, Layout, Metrics, Pose};
use berth::geometry::Rect;
use support::*;

fn dock_at(edge: Edge) -> (Rect, Vec<Rect>) {
    let metrics = Metrics::for_icon(48.0, 1.6).along(edge);
    let lying = metrics.surface(3);
    let (wide, tall) = edge.surface(lying);
    let (screen_wide, screen_tall) = (SCREEN.0 as f32, SCREEN.1 as f32);
    let origin = match edge {
        Edge::Bottom => ((screen_wide - wide) / 2.0, screen_tall - tall),
        Edge::Top => ((screen_wide - wide) / 2.0, 0.0),
        Edge::Left => (0.0, (screen_tall - tall) / 2.0),
        Edge::Right => (screen_wide - wide, (screen_tall - tall) / 2.0),
    };
    let layout = Layout::arrange(&metrics, 3, Pose::RESTING).on(edge, lying);
    let icons = layout
        .icons
        .iter()
        .map(|icon| icon.moved(origin.0, origin.1))
        .collect();
    (layout.panel.moved(origin.0, origin.1), icons)
}

fn the_dock_lives_at(edge: Edge, name: &str, touching: (f32, f32)) {
    let mut desktop = Desktop::with_dock(&format!("[dock]\nedge = \"{name}\"\n{PINNED}"));
    let (panel, icons) = dock_at(edge);
    let all_icons_show = |desktop: &Desktop| {
        let shot = desktop.screenshot();
        [RED, GREEN, BLUE]
            .iter()
            .zip(&icons)
            .all(|(colour, icon)| shot.shows(*colour, icon.center_x(), icon.center_y()))
    };
    eventually("the icons line the chosen edge", || all_icons_show(&desktop));
    let gap_to_the_edge = match edge {
        Edge::Bottom => SCREEN.1 as f32 - panel.bottom(),
        Edge::Top => panel.y,
        Edge::Left => panel.x,
        Edge::Right => SCREEN.0 as f32 - panel.right(),
    };
    assert!((0.0..12.0).contains(&gap_to_the_edge), "{gap_to_the_edge}");

    let green = icons[1];
    let (inward_x, inward_y) = edge.inward(14.0);
    let just_inside = match edge {
        Edge::Bottom => (green.center_x(), green.y + inward_y),
        Edge::Top => (green.center_x(), green.bottom() + inward_y),
        Edge::Left => (green.right() + inward_x, green.center_y()),
        Edge::Right => (green.x + inward_x, green.center_y()),
    };
    desktop.pointer(green.center_x(), green.center_y());
    eventually("the icon under the pointer swells into the screen", || {
        desktop.screenshot().shows(GREEN, just_inside.0, just_inside.1)
    });

    desktop.click(Button::Left);
    eventually("a click starts the app", || {
        desktop.focused().as_deref() == Some("green window")
    });

    desktop.open("red", "big window", "404040", "maximized");
    desktop.pointer(960.0, 540.0);
    eventually("the dock makes way for the window", || {
        desktop.screenshot().count(GREEN) == 0
    });
    desktop.pointer(touching.0, touching.1);
    eventually("and returns at its own edge", || all_icons_show(&desktop));
}

#[test]
fn a_dock_at_the_top() {
    the_dock_lives_at(Edge::Top, "top", (960.0, 0.0));
}

#[test]
fn a_dock_at_the_left() {
    the_dock_lives_at(Edge::Left, "left", (0.0, 540.0));
}

#[test]
fn a_dock_at_the_right() {
    the_dock_lives_at(Edge::Right, "right", (SCREEN.0 as f32 - 1.0, 540.0));
}

#[test]
fn the_menu_of_a_side_dock_opens_beside_it() {
    let mut desktop = Desktop::with_dock("[dock]\nedge = \"left\"\n");
    let (_, icons) = dock_at(Edge::Left);
    eventually("the dock is drawn", || desktop.screenshot().count(RED) > 0);

    desktop.click_at(icons[0].center_x(), icons[0].center_y(), Button::Right);

    let beside = Rect::new(icons[0].right() + 8.0, icons[0].center_y() - 36.0, 150.0, 72.0);
    eventually("the menu is beside the icon", || {
        desktop.screenshot().brightest_in(beside) > 600
    });
    desktop.click_at(beside.x + 40.0, beside.y + 6.0 + 15.0, Button::Left);
    eventually("its first entry opens the app", || {
        desktop.focused().as_deref() == Some("red window")
    });
}
