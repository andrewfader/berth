use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use cosmic_client_toolkit::cosmic_protocols::overlap_notify::v1::client::zcosmic_overlap_notification_v1::{
    self, ZcosmicOverlapNotificationV1,
};
use cosmic_client_toolkit::sctk::compositor::Region;
use cosmic_client_toolkit::sctk::seat::pointer::{
    BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, PointerEvent, PointerEventKind,
};
use cosmic_client_toolkit::sctk::shell::WaylandSurface;
use cosmic_client_toolkit::sctk::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerSurface, SurfaceKind,
};
use tiny_skia::Pixmap;
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1;

use super::Berth;
use super::canvas::{Canvas, Medium};
use crate::animation::Animated;
use crate::dock::actions::{Action, Gesture, effects};
use crate::dock::bounce;
use crate::dock::items::{DockItem, Shown, dock_items};
use crate::dock::layout::{Edge, Layout, Metrics, Pose};
use crate::dock::paint::{CORNER, Scene, Tile, paint};
use crate::dock::reveal::{Autohide, Reveal};
use crate::geometry::Rect;
use crate::paint::rounded_rows;
use crate::windows::{AppId, Windows};

const SWELL: Duration = Duration::from_millis(140);
const SLIDE: Duration = Duration::from_millis(220);
const KEYBOARD_PATIENCE: Duration = Duration::from_millis(150);
const NOTCH: f64 = 15.0;
const EDGE: f32 = 2.0;

struct Face {
    name: String,
    icon_name: Option<String>,
    icon: Option<Rc<Pixmap>>,
    label: Option<Rc<Pixmap>>,
}

pub(super) struct Press {
    index: usize,
    button: u32,
    serial: u32,
    item: DockItem,
    windows: Windows,
}

pub struct DockView {
    output: WlOutput,
    name: Option<String>,
    own_pins: Option<Vec<AppId>>,
    layer: LayerSurface,
    canvas: Canvas,
    blur: Option<ExtBackgroundEffectSurfaceV1>,
    overlap: Option<ZcosmicOverlapNotificationV1>,
    edge: Edge,
    room: f32,
    metrics: Metrics,
    items: Vec<DockItem>,
    look: Vec<(bool, bool, bool)>,
    faces: Vec<Face>,
    size: (u32, u32),
    configured: bool,
    reveal: Reveal,
    shown: Animated,
    zoom: Animated,
    pointer: Option<f32>,
    hovering: bool,
    held_open: bool,
    press: Option<Press>,
    listening: bool,
    keyed: bool,
    awaiting: Option<(Press, Duration)>,
    scrolled: f64,
    overlapping: HashMap<ObjectId, Rect>,
}

impl Drop for DockView {
    fn drop(&mut self) {
        if let Some(overlap) = &self.overlap {
            overlap.destroy();
        }
        if let Some(blur) = &self.blur {
            blur.destroy();
        }
    }
}

impl DockView {
    pub fn is_on(&self, surface: &WlSurface) -> bool {
        self.canvas.surface() == surface
    }

    pub fn is_for(&self, output: &WlOutput) -> bool {
        self.output == *output
    }

    pub fn owns(&self, layer: &LayerSurface) -> bool {
        self.layer.wl_surface() == layer.wl_surface()
    }

    pub fn layer(&self) -> &LayerSurface {
        &self.layer
    }

    pub fn surface(&self) -> &WlSurface {
        self.canvas.surface()
    }

    pub fn hold_open(&mut self) {
        self.held_open = true;
    }

    pub fn resting_icon(&self, index: usize) -> Option<Rect> {
        self.resting().icons.get(index).copied()
    }

    pub fn decides_its_own_pins(&self) -> bool {
        self.own_pins.is_some()
    }

    pub fn edge(&self) -> Edge {
        self.edge
    }

    fn lying(&self) -> (f32, f32) {
        self.metrics.surface(self.items.len())
    }

    fn resting(&self) -> Layout {
        Layout::arrange(&self.metrics, self.items.len(), Pose::RESTING).on(self.edge, self.lying())
    }

    pub fn item(&self, index: usize) -> Option<&DockItem> {
        self.items.get(index)
    }

    fn fit(&mut self, wanted_icon: f32, magnification: f32) {
        let metrics =
            Metrics::fitted(wanted_icon, magnification, self.items.len(), self.room).along(self.edge);
        if metrics.icon != self.metrics.icon {
            self.forget_pictures();
        }
        self.metrics = metrics;
        let (wide, tall) = self.edge.surface(self.lying());
        let size = (wide as u32, tall as u32);
        if size != self.size {
            self.size = size;
            self.configured = false;
            self.layer.set_size(size.0, size.1);
            self.layer.commit();
        }
    }

    fn forget_pictures(&mut self) {
        for face in &mut self.faces {
            face.icon = None;
            face.label = None;
        }
    }

    fn resting_panel(&self) -> Rect {
        self.resting().panel
    }

    fn is_obstructed(&self) -> bool {
        let panel = self.resting_panel();
        self.overlapping.values().any(|window| window.intersects(&panel))
    }

    fn stop_listening(&mut self) {
        if self.listening {
            self.layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            self.layer.commit();
        }
        self.listening = false;
        self.keyed = false;
    }
}

impl Berth {
    pub(super) fn open_dock(&mut self, output: WlOutput) {
        if self.docks.iter().any(|dock| dock.is_for(&output)) {
            return;
        }
        let config = &self.config.dock;
        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(
            &self.qh,
            surface.clone(),
            Layer::Top,
            Some("berth-dock"),
            Some(&output),
        );
        let edge = config.edge;
        let metrics = Metrics::for_icon(config.icon_size as f32, config.magnification).along(edge);
        layer.set_anchor(match edge {
            Edge::Bottom => Anchor::BOTTOM,
            Edge::Top => Anchor::TOP,
            Edge::Left => Anchor::LEFT,
            Edge::Right => Anchor::RIGHT,
        });
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_exclusive_zone(match config.autohide {
            Autohide::Never => metrics.travel() as i32,
            Autohide::Always | Autohide::Intelligent => 0,
        });
        let blur = self
            .extras
            .blur
            .as_ref()
            .filter(|_| config.blur)
            .map(|manager| manager.get_background_effect(&surface, &self.qh, ()));

        let name = self.outputs.info(&output).and_then(|info| info.name);
        let own_pins = name
            .as_ref()
            .and_then(|name| config.monitor.get(name))
            .and_then(|monitor| monitor.pinned.as_ref())
            .map(|wanted| self.catalog.pinned(wanted));
        self.docks.push(DockView {
            name,
            own_pins,
            edge,
            room: self.room_on(&output, edge),
            output,
            canvas: Canvas::on(surface, &self.extras, &self.qh),
            layer,
            blur,
            overlap: None,
            metrics,
            items: Vec::new(),
            look: Vec::new(),
            faces: Vec::new(),
            size: (0, 0),
            configured: false,
            reveal: Reveal::new(config.autohide),
            shown: Animated::resting_at(1.0),
            zoom: Animated::resting_at(0.0),
            pointer: None,
            hovering: false,
            held_open: false,
            press: None,
            listening: false,
            keyed: false,
            awaiting: None,
            scrolled: 0.0,
            overlapping: HashMap::new(),
        });
        self.refresh_dock_items();
    }

    pub(super) fn close_docks(&mut self, gone: impl Fn(&DockView) -> bool) {
        if self
            .menu
            .as_ref()
            .is_some_and(|menu| self.docks.iter().any(|dock| gone(dock) && menu.belongs_to(dock)))
        {
            self.menu = None;
        }
        self.docks.retain(|dock| !gone(dock));
    }

    pub(super) fn output_changed(&mut self, output: &WlOutput) {
        let edge = self.config.dock.edge;
        let room = self.room_on(output, edge);
        if let Some(dock) = self.docks.iter_mut().find(|dock| dock.is_for(output)) {
            dock.room = room;
        }
        self.refresh_dock_items();
    }

    fn room_on(&self, output: &WlOutput, edge: Edge) -> f32 {
        self.outputs
            .info(output)
            .and_then(|info| {
                let scaled = info.modes.iter().find(|mode| mode.current).map(|mode| {
                    (
                        mode.dimensions.0 / info.scale_factor.max(1),
                        mode.dimensions.1 / info.scale_factor.max(1),
                    )
                });
                info.logical_size.or(scaled)
            })
            .map_or(f32::INFINITY, |(wide, tall)| edge.along(wide as f32, tall as f32))
    }

    pub(super) fn refresh_dock_items(&mut self) {
        let now = self.now();
        let (wanted_icon, magnification) =
            (self.config.dock.icon_size as f32, self.config.dock.magnification);
        let own_monitor_only = self.config.dock.windows == Shown::ThisMonitor;
        for dock in &mut self.docks {
            let pinned = dock.own_pins.as_deref().unwrap_or(self.pins.apps());
            let monitor = dock.name.as_deref().filter(|_| own_monitor_only);
            let items = dock_items(pinned, &self.windows, monitor);
            let same_apps = items
                .iter()
                .map(|item| &item.app)
                .eq(dock.items.iter().map(|item| &item.app));
            if !same_apps {
                dock.faces = items
                    .iter()
                    .map(|item| Face {
                        name: self.catalog.name(&item.app),
                        icon_name: self.catalog.icon(&item.app).map(str::to_owned),
                        icon: None,
                        label: None,
                    })
                    .collect();
            }
            let look: Vec<(bool, bool, bool)> = items
                .iter()
                .map(|item| {
                    (
                        item.is_running(),
                        item.active_window(&self.windows).is_some(),
                        self.launching.contains_key(&item.app),
                    )
                })
                .collect();
            let looks_the_same = same_apps && look == dock.look;
            dock.items = items;
            dock.look = look;
            dock.fit(wanted_icon, magnification);
            let obstructed = dock.is_obstructed();
            dock.reveal.obstruction_changed(obstructed, now);
            if !looks_the_same || !dock.configured {
                dock.canvas.invalidate();
            }
        }
    }

    pub(super) fn dock_at(&self, surface: &WlSurface) -> Option<usize> {
        self.docks.iter().position(|dock| dock.is_on(surface))
    }

    pub(super) fn dock_configured(&mut self, layer: &LayerSurface, size: (u32, u32)) {
        let Some(dock) = self.docks.iter_mut().find(|dock| dock.owns(layer)) else {
            return;
        };
        if size.0 > 0 && size.1 > 0 {
            dock.size = size;
        }
        dock.configured = true;
        dock.canvas.invalidate();
    }

    pub(super) fn dock_frame(&mut self, surface: &WlSurface) {
        if let Some(dock) = self.docks.iter_mut().find(|dock| dock.is_on(surface)) {
            dock.canvas.frame_arrived();
        }
    }

    pub(super) fn rescale_dock(&mut self, surface: &WlSurface, scale120: u32, fractional: bool) {
        if let Some(dock) = self.docks.iter_mut().find(|dock| dock.is_on(surface))
            && dock.canvas.follows_fractional_scale() == fractional
            && dock.canvas.rescale(scale120)
        {
            dock.forget_pictures();
        }
    }

    pub(super) fn dock_deadline(&self) -> Option<Duration> {
        self.docks
            .iter()
            .flat_map(|dock| {
                let hides = dock.reveal.hides_at().filter(|_| dock.shown.target() > 0.0);
                let decides = dock.awaiting.as_ref().map(|(_, deadline)| *deadline);
                [hides, decides]
            })
            .flatten()
            .min()
    }

    pub(super) fn settle_docks(&mut self) {
        let now = self.now();
        self.launching
            .retain(|_, started| now.saturating_sub(*started) < bounce::PATIENCE);
        let slide = self.config.animation.lasting(SLIDE);
        for at in 0..self.docks.len() {
            let dock = &mut self.docks[at];
            let wanted = if dock.reveal.is_shown(now) { 1.0 } else { 0.0 };
            if dock.shown.target() != wanted {
                dock.shown.head_for(wanted, slide, now);
                dock.canvas.invalidate();
            }
            if dock
                .awaiting
                .as_ref()
                .is_some_and(|(_, deadline)| now >= *deadline)
                && let Some((press, _)) = dock.awaiting.take()
            {
                self.click(at, press);
            }
        }
    }

    pub(super) fn paint_docks(&mut self) {
        for at in 0..self.docks.len() {
            self.paint_dock(at);
        }
    }

    fn paint_dock(&mut self, at: usize) {
        let now = self.now();
        let dock = &mut self.docks[at];
        if !dock.configured || !dock.canvas.wants_paint() {
            return;
        }
        let (shown, zoom) = (dock.shown.value(now), dock.zoom.value(now));
        let pose = Pose {
            pointer: dock.pointer.filter(|_| zoom > 0.0),
            zoom,
            shown,
        };
        let (edge, lying) = (dock.edge, dock.lying());
        let flat = Layout::arrange(&dock.metrics, dock.items.len(), pose);
        let layout = flat.on(edge, lying);
        let scale = dock.canvas.scale();
        let pixels = (dock.metrics.icon * dock.metrics.magnification * scale).ceil() as u32;

        for face in &mut dock.faces {
            if face.icon.is_none() {
                face.icon = Some(self.icons.icon(
                    face.icon_name.as_deref(),
                    &face.name,
                    pixels,
                    &mut self.typesetter,
                ));
            }
        }
        let hovered = dock
            .pointer
            .filter(|_| dock.hovering && shown >= 1.0)
            .and_then(|x| Layout::item_at(&dock.metrics, dock.items.len(), x));
        if let Some(face) = hovered.and_then(|index| dock.faces.get_mut(index))
            && face.label.is_none()
        {
            face.label = self
                .typesetter
                .line(&face.name, 13.0 * scale, 360.0 * scale)
                .map(Rc::new);
        }

        let hops = self.config.animation.bounces_on_launch();
        let tiles: Vec<Tile> = dock
            .items
            .iter()
            .zip(&dock.faces)
            .filter_map(|(item, face)| {
                let hopping_since = self.launching.get(&item.app).filter(|_| hops);
                Some(Tile {
                    icon: face.icon.as_deref()?,
                    running: item.is_running(),
                    focused: item.active_window(&self.windows).is_some(),
                    lift: hopping_since.map_or(0.0, |started| {
                        bounce::lift(now.saturating_sub(*started), dock.metrics.icon)
                    }),
                })
            })
            .collect();
        let label = hovered.and_then(|index| Some((index, dock.faces.get(index)?.label.as_deref()?)));

        let surface = dock.canvas.surface().clone();
        if let Ok(region) = Region::new(&self.compositor) {
            let touchable = if shown <= 0.0 {
                let panel = Layout::arrange(&dock.metrics, dock.items.len(), Pose::RESTING).panel;
                Rect::new(panel.x, lying.1 - EDGE, panel.width, EDGE)
            } else {
                let reach = flat.reach();
                Rect::new(
                    reach.x.floor(),
                    reach.y.floor(),
                    reach.width.ceil() + 1.0,
                    lying.1 - reach.y.floor(),
                )
            };
            let touchable = edge.place(touchable, lying);
            region.add(
                touchable.x as i32,
                touchable.y as i32,
                touchable.width as i32,
                touchable.height as i32,
            );
            surface.set_input_region(Some(region.wl_region()));
        }
        if let Some(blur) = &dock.blur
            && let Ok(region) = Region::new(&self.compositor)
        {
            for (x, y, wide, tall) in rounded_rows(layout.panel, CORNER) {
                region.add(x, y, wide, tall);
            }
            blur.set_blur_region(Some(region.wl_region()));
        }

        let hopping = hops
            && dock
                .items
                .iter()
                .any(|item| self.launching.contains_key(&item.app));
        let animating = hopping || !dock.shown.is_settled(now) || !dock.zoom.is_settled(now);
        let scene = Scene {
            layout: &layout,
            tiles: &tiles,
            label,
            edge,
            opacity: self.config.dock.opacity,
            scale,
        };
        dock.canvas.present(
            Medium {
                pool: &mut self.pool,
                format: self.format,
                qh: &self.qh,
            },
            dock.size,
            1.0,
            animating,
            |canvas, scale| paint(canvas, &Scene { scale, ..scene }),
        );
        if dock.overlap.is_none()
            && let Some(notify) = &self.extras.overlap
            && let SurfaceKind::Wlr(mapped) = dock.layer.kind()
        {
            dock.overlap = Some(notify.notify_on_overlap(mapped, &self.qh, ()));
        }
    }

    pub(super) fn dock_pointer(&mut self, at: usize, event: &PointerEvent) {
        let now = self.now();
        let swell = self.config.animation.lasting(SWELL);
        let watches_modifiers = self.config.dock.actions.distinguish_modifiers();
        let dock = &mut self.docks[at];
        let x = dock.edge.along(event.position.0 as f32, event.position.1 as f32);
        let pointed = Layout::item_at(&dock.metrics, dock.items.len(), x);
        dock.canvas.invalidate();
        match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                dock.pointer = Some(x);
                dock.hovering = true;
                dock.reveal.pointer_entered(now);
                dock.zoom.head_for(1.0, swell, now);
            }
            PointerEventKind::Leave { .. } => {
                dock.hovering = false;
                dock.press = None;
                dock.scrolled = 0.0;
                dock.zoom.head_for(0.0, swell, now);
                if dock.awaiting.is_none() {
                    dock.stop_listening();
                }
                if !dock.held_open {
                    dock.reveal.pointer_left(now);
                }
            }
            PointerEventKind::Press { button, serial, .. } => {
                dock.press = pointed.and_then(|index| {
                    Some(Press {
                        index,
                        button,
                        serial,
                        item: dock.items.get(index)?.clone(),
                        windows: self.windows.clone(),
                    })
                });
                if button == BTN_LEFT && watches_modifiers && dock.press.is_some() {
                    dock.listening = true;
                    dock.keyed = false;
                    dock.layer
                        .set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
                    dock.layer.commit();
                }
                self.close_menu();
            }
            PointerEventKind::Release { button, .. } => {
                let clicked = dock
                    .press
                    .take()
                    .filter(|press| Some(press.index) == pointed && press.button == button);
                match clicked {
                    Some(press) if dock.listening && !dock.keyed => {
                        dock.awaiting = Some((press, now + KEYBOARD_PATIENCE));
                    }
                    Some(press) => self.click(at, press),
                    None => dock.stop_listening(),
                }
            }
            PointerEventKind::Axis { vertical, .. } => {
                dock.scrolled += if vertical.value120 != 0 {
                    vertical.value120 as f64 / 120.0 * NOTCH
                } else {
                    vertical.absolute
                };
                let notches = (dock.scrolled / NOTCH).trunc();
                dock.scrolled -= notches * NOTCH;
                let gesture = if notches > 0.0 {
                    Gesture::ScrollDown
                } else {
                    Gesture::ScrollUp
                };
                if let Some(item) = pointed.and_then(|index| dock.items.get(index)).cloned()
                    && notches != 0.0
                {
                    let action = self.config.dock.actions.action_for(gesture);
                    for effect in effects(action, &item, &self.windows) {
                        self.perform(effect);
                    }
                }
            }
        }
    }

    pub(super) fn dock_focused(&mut self, surface: &WlSurface) {
        if let Some(dock) = self.docks.iter_mut().find(|dock| dock.is_on(surface)) {
            dock.keyed = dock.listening;
        }
    }

    pub(super) fn dock_modifiers(&mut self) {
        for at in 0..self.docks.len() {
            if self.docks[at].keyed
                && let Some((press, _)) = self.docks[at].awaiting.take()
            {
                self.click(at, press);
            }
        }
    }

    pub(super) fn forget_overlap(&mut self, toplevel: &ExtForeignToplevelHandleV1) {
        for dock in &mut self.docks {
            dock.overlapping.remove(&toplevel.id());
        }
    }

    pub(super) fn release_dock(&mut self, surface: &WlSurface) {
        let now = self.now();
        if let Some(dock) = self.docks.iter_mut().find(|dock| dock.is_on(surface)) {
            dock.held_open = false;
            if !dock.hovering {
                dock.reveal.pointer_left(now);
            }
        }
    }

    fn click(&mut self, at: usize, press: Press) {
        let dock = &mut self.docks[at];
        let gesture = match press.button {
            BTN_LEFT if dock.keyed && self.modifiers.shift => Gesture::ShiftClick,
            BTN_LEFT if dock.keyed && self.modifiers.ctrl => Gesture::CtrlClick,
            BTN_LEFT => Gesture::Click,
            BTN_MIDDLE => Gesture::MiddleClick,
            BTN_RIGHT => Gesture::RightClick,
            _ => return dock.stop_listening(),
        };
        dock.stop_listening();
        match self.config.dock.actions.action_for(gesture) {
            Action::Menu => self.open_menu(at, press.index, press.serial),
            action => {
                for effect in effects(action, &press.item, &press.windows) {
                    self.perform(effect);
                }
            }
        }
    }
}

impl Dispatch<ZcosmicOverlapNotificationV1, ()> for Berth {
    fn event(
        berth: &mut Self,
        notification: &ZcosmicOverlapNotificationV1,
        event: zcosmic_overlap_notification_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let now = berth.now();
        let Some(dock) = berth
            .docks
            .iter_mut()
            .find(|dock| dock.overlap.as_ref() == Some(notification))
        else {
            return;
        };
        match event {
            zcosmic_overlap_notification_v1::Event::ToplevelEnter {
                toplevel,
                x,
                y,
                width,
                height,
            } => {
                let covered = Rect::new(x as f32, y as f32, width as f32, height as f32);
                dock.overlapping.insert(toplevel.id(), covered);
            }
            zcosmic_overlap_notification_v1::Event::ToplevelLeave { toplevel } => {
                dock.overlapping.remove(&toplevel.id());
            }
            _ => return,
        }
        let obstructed = dock.is_obstructed();
        dock.reveal.obstruction_changed(obstructed, now);
    }
}
