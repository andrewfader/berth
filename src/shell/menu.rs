use std::rc::Rc;
use std::time::Duration;

use cosmic_client_toolkit::sctk::seat::pointer::{BTN_LEFT, PointerEvent, PointerEventKind};
use cosmic_client_toolkit::sctk::shell::xdg::XdgPositioner;
use cosmic_client_toolkit::sctk::shell::xdg::popup::Popup;
use tiny_skia::Pixmap;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_protocols::xdg::shell::client::xdg_positioner::{Anchor, ConstraintAdjustment, Gravity};

use super::Berth;
use super::canvas::{Canvas, Medium};
use super::dock::DockView;
use crate::dock::menu::{self, Entry, Scene};

const GAP: i32 = 8;
const TEXT: f32 = 13.0;
const LINGER: Duration = Duration::from_millis(500);

pub struct MenuView {
    popup: Popup,
    canvas: Canvas,
    owner: WlSurface,
    entries: Vec<Entry>,
    labels: Vec<Option<Rc<Pixmap>>>,
    size: (u32, u32),
    configured: bool,
    pointed: Option<usize>,
    hovered: bool,
    abandoned_at: Option<Duration>,
}

impl MenuView {
    pub fn belongs_to(&self, dock: &DockView) -> bool {
        dock.is_on(&self.owner)
    }

    pub fn is_on(&self, surface: &WlSurface) -> bool {
        self.canvas.surface() == surface
    }
}

impl Berth {
    pub(super) fn open_menu(&mut self, at: usize, index: usize) {
        self.close_menu();
        let Some(dock) = self.docks.get(at) else {
            return;
        };
        let (Some(item), Some(icon)) = (dock.item(index), dock.resting_icon(index)) else {
            return;
        };
        let entries = menu::entries(item, &self.windows, self.pins.are_editable());
        let widest = entries
            .iter()
            .filter_map(|entry| self.typesetter.line(&entry.label, TEXT, menu::WIDEST))
            .map(|label| label.width() as f32)
            .fold(0.0, f32::max);
        let (wide, tall) = menu::size(entries.len(), widest);

        let Ok(positioner) = XdgPositioner::new(&self.xdg_shell) else {
            return;
        };
        positioner.set_size(wide as i32, tall as i32);
        positioner.set_anchor_rect(
            icon.x as i32,
            icon.y as i32,
            icon.width as i32,
            icon.height as i32,
        );
        positioner.set_anchor(Anchor::Top);
        positioner.set_gravity(Gravity::Top);
        positioner.set_offset(0, -GAP);
        positioner.set_constraint_adjustment(ConstraintAdjustment::SlideX);
        let surface = self.compositor.create_surface(&self.qh);
        let Ok(popup) = Popup::from_surface(None, &positioner, &self.qh, surface.clone(), &self.xdg_shell)
        else {
            return;
        };
        dock.layer().get_popup(popup.xdg_popup());
        surface.commit();

        self.docks[at].hold_open();
        self.menu = Some(MenuView {
            popup,
            owner: self.docks[at].surface().clone(),
            canvas: Canvas::on(surface, &self.extras, &self.qh),
            labels: vec![None; entries.len()],
            entries,
            size: (wide as u32, tall as u32),
            configured: false,
            pointed: None,
            hovered: false,
            abandoned_at: None,
        });
    }

    pub(super) fn close_menu(&mut self) {
        if let Some(menu) = self.menu.take() {
            self.release_dock(&menu.owner);
        }
    }

    pub(super) fn menu_configured(&mut self, popup: &Popup) {
        if let Some(menu) = self.menu.as_mut().filter(|menu| menu.popup == *popup) {
            menu.configured = true;
            menu.canvas.invalidate();
        }
    }

    pub(super) fn menu_dismissed(&mut self, popup: &Popup) {
        if self.menu.as_ref().is_some_and(|menu| menu.popup == *popup) {
            self.close_menu();
        }
    }

    pub(super) fn menu_frame(&mut self, surface: &WlSurface) {
        if let Some(menu) = self.menu.as_mut().filter(|menu| menu.is_on(surface)) {
            menu.canvas.frame_arrived();
        }
    }

    pub(super) fn rescale_menu(&mut self, surface: &WlSurface, scale120: u32, fractional: bool) {
        if let Some(menu) = self.menu.as_mut().filter(|menu| menu.is_on(surface))
            && menu.canvas.follows_fractional_scale() == fractional
            && menu.canvas.rescale(scale120)
        {
            menu.labels.fill(None);
        }
    }

    pub(super) fn menu_deadline(&self) -> Option<Duration> {
        self.menu.as_ref()?.abandoned_at.map(|since| since + LINGER)
    }

    pub(super) fn settle_menu(&mut self) {
        let now = self.now();
        let Some(menu) = &mut self.menu else {
            return;
        };
        let attended = menu.hovered
            || self
                .docks
                .iter()
                .any(|dock| menu.belongs_to(dock) && dock.is_hovered());
        menu.abandoned_at = match (attended, menu.abandoned_at) {
            (true, _) => None,
            (false, None) => Some(now),
            (false, since) => since,
        };
        if menu.abandoned_at.is_some_and(|since| now >= since + LINGER) {
            self.close_menu();
        }
    }

    pub(super) fn paint_menu(&mut self) {
        let Some(menu) = &mut self.menu else {
            return;
        };
        if !menu.configured || !menu.canvas.wants_paint() {
            return;
        }
        let scale = menu.canvas.scale();
        let room = menu.size.0 as f32 - 2.0 * (menu::MARGIN + menu::INSET);
        for (label, entry) in menu.labels.iter_mut().zip(&menu.entries) {
            if label.is_none() {
                *label = self
                    .typesetter
                    .line(&entry.label, TEXT * scale, room * scale)
                    .map(Rc::new);
            }
        }
        let labels: Vec<Option<&Pixmap>> = menu.labels.iter().map(Option::as_deref).collect();
        let pointed = menu.pointed;
        menu.canvas.present(
            Medium {
                pool: &mut self.pool,
                format: self.format,
                qh: &self.qh,
            },
            menu.size,
            1.0,
            false,
            |canvas, scale| {
                menu::paint(
                    canvas,
                    &Scene {
                        labels: &labels,
                        pointed,
                        scale,
                    },
                )
            },
        );
    }

    pub(super) fn menu_pointer(&mut self, event: &PointerEvent) {
        let Some(menu) = &mut self.menu else {
            return;
        };
        let row = menu::row_at(menu.entries.len(), event.position.1 as f32);
        menu.canvas.invalidate();
        match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                menu.hovered = true;
                menu.pointed = row;
            }
            PointerEventKind::Leave { .. } => {
                menu.hovered = false;
                menu.pointed = None;
            }
            PointerEventKind::Release { button: BTN_LEFT, .. } => {
                if let Some(entry) = row.and_then(|row| menu.entries.get(row)).cloned() {
                    self.close_menu();
                    for effect in entry.effects {
                        self.perform(effect);
                    }
                }
            }
            _ => {}
        }
    }
}
