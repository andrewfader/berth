use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use cosmic_client_toolkit::cosmic_protocols::overlap_notify::v1::client::zcosmic_overlap_notification_v1::{
    self, ZcosmicOverlapNotificationV1,
};
use cosmic_client_toolkit::sctk::compositor::{CompositorState, Region};
use cosmic_client_toolkit::sctk::seat::pointer::{
    BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, PointerEvent, PointerEventKind,
};
use cosmic_client_toolkit::sctk::shell::WaylandSurface;
use cosmic_client_toolkit::sctk::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface, SurfaceKind,
};
use tiny_skia::Pixmap;
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1;

use super::canvas::{Canvas, Medium};
use super::{Berth, Extras};
use crate::animation::Animated;
use crate::apps::Catalog;
use crate::config::{Config, config_home};
use crate::cosmic;
use crate::dock::actions::{Gesture, effects};
use crate::dock::items::{DockItem, dock_items};
use crate::dock::layout::{Layout, Metrics, Pose};
use crate::dock::paint::{CORNER, Scene, Tile, paint};
use crate::dock::reveal::{Autohide, Reveal};
use crate::geometry::Rect;
use crate::paint::rounded_rows;
use crate::windows::AppId;

const SWELL: Duration = Duration::from_millis(140);
const SLIDE: Duration = Duration::from_millis(220);
const NOTCH: f64 = 15.0;
const EDGE: i32 = 2;

struct Face {
    name: String,
    icon_name: Option<String>,
    icon: Option<Rc<Pixmap>>,
    label: Option<Rc<Pixmap>>,
}

pub struct DockView {
    layer: LayerSurface,
    canvas: Canvas,
    blur: Option<ExtBackgroundEffectSurfaceV1>,
    overlap: Option<ZcosmicOverlapNotificationV1>,
    metrics: Metrics,
    pinned: Vec<AppId>,
    items: Vec<DockItem>,
    faces: Vec<Face>,
    size: (u32, u32),
    configured: bool,
    reveal: Reveal,
    shown: Animated,
    zoom: Animated,
    pointer: Option<f32>,
    hovering: bool,
    pressed: Option<(usize, u32)>,
    scrolled: f64,
    overlapping: HashMap<ObjectId, Rect>,
}

impl DockView {
    pub fn open(
        config: &Config,
        catalog: &Catalog,
        compositor: &CompositorState,
        layer_shell: &LayerShell,
        extras: &Extras,
        qh: &QueueHandle<Berth>,
    ) -> Self {
        let surface = compositor.create_surface(qh);
        let layer =
            layer_shell.create_layer_surface(qh, surface.clone(), Layer::Top, Some("berth-dock"), None);
        let metrics = Metrics::for_icon(config.dock.icon_size as f32, config.dock.magnification);
        layer.set_anchor(Anchor::BOTTOM);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_exclusive_zone(match config.dock.autohide {
            Autohide::Never => metrics.travel() as i32,
            Autohide::Always | Autohide::Intelligent => 0,
        });

        let blur = extras
            .blur
            .as_ref()
            .filter(|_| config.dock.blur)
            .map(|manager| manager.get_background_effect(&surface, qh, ()));
        let wanted = config
            .dock
            .pinned
            .clone()
            .or_else(|| cosmic::setting(&config_home(), cosmic::FAVORITES))
            .unwrap_or_default();
        let pinned = catalog.pinned(&wanted);
        let mut dock = Self {
            canvas: Canvas::on(surface, extras, qh),
            layer,
            blur,
            overlap: None,
            metrics,
            pinned,
            items: Vec::new(),
            faces: Vec::new(),
            size: (0, 0),
            configured: false,
            reveal: Reveal::new(config.dock.autohide),
            shown: Animated::resting_at(1.0),
            zoom: Animated::resting_at(0.0),
            pointer: None,
            hovering: false,
            pressed: None,
            scrolled: 0.0,
            overlapping: HashMap::new(),
        };
        dock.resize();
        dock
    }

    pub fn is_on(&self, surface: &WlSurface) -> bool {
        self.canvas.surface() == surface
    }

    fn resize(&mut self) {
        let (wide, tall) = self.metrics.surface(self.items.len());
        let size = (wide as u32, tall as u32);
        if size != self.size {
            self.size = size;
            self.configured = false;
            self.layer.set_size(size.0, size.1);
            self.layer.commit();
        }
    }

    fn resting_panel(&self) -> Rect {
        Layout::arrange(&self.metrics, self.items.len(), Pose::RESTING).panel
    }

    fn is_obstructed(&self) -> bool {
        let panel = self.resting_panel();
        self.overlapping.values().any(|window| window.intersects(&panel))
    }
}

impl Berth {
    pub(super) fn refresh_dock_items(&mut self) {
        let items = dock_items(&self.dock.pinned, &self.windows);
        let same_apps = items
            .iter()
            .map(|item| &item.app)
            .eq(self.dock.items.iter().map(|item| &item.app));
        if !same_apps {
            self.dock.faces = items
                .iter()
                .map(|item| Face {
                    name: self.catalog.name(&item.app),
                    icon_name: self.catalog.icon(&item.app).map(str::to_owned),
                    icon: None,
                    label: None,
                })
                .collect();
        }
        self.dock.items = items;
        self.dock.resize();
        let now = self.now();
        let obstructed = self.dock.is_obstructed();
        self.dock.reveal.obstruction_changed(obstructed, now);
        self.dock.canvas.invalidate();
    }

    pub(super) fn dock_configured(&mut self, size: (u32, u32)) {
        if size.0 > 0 && size.1 > 0 {
            self.dock.size = size;
        }
        self.dock.configured = true;
        self.dock.canvas.invalidate();
    }

    pub(super) fn dock_frame(&mut self, surface: &WlSurface) {
        if self.dock.is_on(surface) {
            self.dock.canvas.frame_arrived();
        }
    }

    pub(super) fn rescale_dock(&mut self, surface: &WlSurface, scale120: u32, fractional: bool) {
        let dock = &mut self.dock;
        if dock.is_on(surface)
            && dock.canvas.follows_fractional_scale() == fractional
            && dock.canvas.rescale(scale120)
        {
            for face in &mut dock.faces {
                face.icon = None;
                face.label = None;
            }
        }
    }

    pub(super) fn dock_deadline(&self) -> Option<Duration> {
        self.dock
            .reveal
            .hides_at()
            .filter(|_| self.dock.shown.target() > 0.0)
    }

    pub(super) fn settle_dock(&mut self) {
        let now = self.now();
        let dock = &mut self.dock;
        let wanted = if dock.reveal.is_shown(now) { 1.0 } else { 0.0 };
        if dock.shown.target() != wanted {
            dock.shown.head_for(wanted, SLIDE, now);
            dock.canvas.invalidate();
        }
    }

    pub(super) fn paint_dock(&mut self) {
        let now = self.now();
        let dock = &mut self.dock;
        if !dock.configured || !dock.canvas.wants_paint() {
            return;
        }
        let (shown, zoom) = (dock.shown.value(now), dock.zoom.value(now));
        let pose = Pose {
            pointer: dock.pointer.filter(|_| zoom > 0.0),
            zoom,
            shown,
        };
        let layout = Layout::arrange(&dock.metrics, dock.items.len(), pose);
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

        let tiles: Vec<Tile> = dock
            .items
            .iter()
            .zip(&dock.faces)
            .filter_map(|(item, face)| {
                Some(Tile {
                    icon: face.icon.as_deref()?,
                    running: item.is_running(),
                    focused: item.active_window(&self.windows).is_some(),
                })
            })
            .collect();
        let label = hovered.and_then(|index| Some((index, dock.faces.get(index)?.label.as_deref()?)));

        let surface = dock.canvas.surface().clone();
        if let Ok(region) = Region::new(&self.compositor) {
            if shown <= 0.0 {
                let panel = dock.resting_panel();
                region.add(
                    panel.x as i32,
                    dock.size.1 as i32 - EDGE,
                    panel.width as i32,
                    EDGE,
                );
            } else {
                let reach = layout.reach();
                let top = reach.y.floor() as i32;
                region.add(
                    reach.x.floor() as i32,
                    top,
                    reach.width.ceil() as i32 + 1,
                    dock.size.1 as i32 - top,
                );
            }
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

        let animating = !dock.shown.is_settled(now) || !dock.zoom.is_settled(now);
        let scene = Scene {
            layout: &layout,
            tiles: &tiles,
            label,
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

    pub(super) fn dock_pointer(&mut self, event: &PointerEvent) {
        let now = self.now();
        let x = event.position.0 as f32;
        let dock = &mut self.dock;
        let pointed = Layout::item_at(&dock.metrics, dock.items.len(), x);
        dock.canvas.invalidate();
        match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                dock.pointer = Some(x);
                dock.hovering = true;
                dock.reveal.pointer_entered(now);
                dock.zoom.head_for(1.0, SWELL, now);
            }
            PointerEventKind::Leave { .. } => {
                dock.hovering = false;
                dock.pressed = None;
                dock.scrolled = 0.0;
                dock.reveal.pointer_left(now);
                dock.zoom.head_for(0.0, SWELL, now);
            }
            PointerEventKind::Press { button, .. } => dock.pressed = pointed.map(|index| (index, button)),
            PointerEventKind::Release { button, .. } => {
                let clicked = dock
                    .pressed
                    .take()
                    .filter(|pressed| Some(*pressed) == pointed.map(|index| (index, button)));
                let gesture = clicked.and_then(|(index, button)| {
                    let gesture = match button {
                        BTN_LEFT => Gesture::Click,
                        BTN_MIDDLE => Gesture::MiddleClick,
                        BTN_RIGHT => Gesture::RightClick,
                        _ => return None,
                    };
                    Some((index, gesture))
                });
                if let Some((index, gesture)) = gesture {
                    self.gesture(index, gesture);
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
                if let Some(index) = pointed
                    && notches != 0.0
                {
                    self.gesture(
                        index,
                        if notches > 0.0 {
                            Gesture::ScrollDown
                        } else {
                            Gesture::ScrollUp
                        },
                    );
                }
            }
        }
    }

    pub(super) fn forget_overlap(&mut self, toplevel: &ExtForeignToplevelHandleV1) {
        self.dock.overlapping.remove(&toplevel.id());
    }

    fn gesture(&mut self, index: usize, gesture: Gesture) {
        let Some(item) = self.dock.items.get(index) else {
            return;
        };
        let action = self.config.dock.actions.action_for(gesture);
        for effect in effects(action, item, &self.windows) {
            self.perform(effect);
        }
    }
}

impl Dispatch<ZcosmicOverlapNotificationV1, ()> for Berth {
    fn event(
        berth: &mut Self,
        _: &ZcosmicOverlapNotificationV1,
        event: zcosmic_overlap_notification_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zcosmic_overlap_notification_v1::Event::ToplevelEnter {
                toplevel,
                x,
                y,
                width,
                height,
            } => {
                let covered = Rect::new(x as f32, y as f32, width as f32, height as f32);
                berth.dock.overlapping.insert(toplevel.id(), covered);
            }
            zcosmic_overlap_notification_v1::Event::ToplevelLeave { toplevel } => {
                berth.dock.overlapping.remove(&toplevel.id());
            }
            _ => return,
        }
        let now = berth.now();
        let obstructed = berth.dock.is_obstructed();
        log::debug!(
            "windows over the dock: {:?}, in its way: {obstructed}",
            berth.dock.overlapping.values()
        );
        berth.dock.reveal.obstruction_changed(obstructed, now);
    }
}
