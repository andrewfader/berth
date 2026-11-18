use cosmic_client_toolkit::sctk::seat::pointer::{PointerEvent, PointerEventKind};
use cosmic_client_toolkit::sctk::shell::WaylandSurface;
use cosmic_client_toolkit::sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerSurface};
use cosmic_client_toolkit::sctk::shm::slot::Buffer;
use wayland_client::protocol::{wl_output::WlOutput, wl_surface::WlSurface};

use super::Berth;
use crate::geometry::Rect;
use crate::overview::corner::HotCorner;

pub struct CornerView {
    output: WlOutput,
    layer: LayerSurface,
    buffer: Option<Buffer>,
}

impl CornerView {
    pub fn owns(&self, layer: &LayerSurface) -> bool {
        self.layer.wl_surface() == layer.wl_surface()
    }

    pub fn is_on(&self, surface: &WlSurface) -> bool {
        self.layer.wl_surface() == surface
    }
}

impl Berth {
    pub(super) fn hot_monitors(&self, gone: Option<&WlOutput>) -> Vec<WlOutput> {
        let corner = self.config.overview.hot_corner;
        let monitors: Vec<(WlOutput, Rect)> = self
            .outputs
            .outputs()
            .filter(|output| Some(output) != gone)
            .filter_map(|output| {
                let info = self.outputs.info(&output)?;
                let ((x, y), (wide, tall)) = (info.logical_position?, info.logical_size?);
                Some((output, Rect::new(x as f32, y as f32, wide as f32, tall as f32)))
            })
            .collect();
        let areas: Vec<Rect> = monitors.iter().map(|(_, area)| *area).collect();
        monitors
            .into_iter()
            .filter(|(_, area)| corner.is_hot_on(*area, &areas))
            .map(|(output, _)| output)
            .collect()
    }

    pub(super) fn place_corners(&mut self, gone: Option<&WlOutput>) {
        self.corners.clear();
        if self.overview.is_some() {
            return;
        }
        for output in self.hot_monitors(gone) {
            let side = match self.config.overview.hot_corner {
                HotCorner::TopRight => Anchor::RIGHT,
                _ => Anchor::LEFT,
            };
            let surface = self.compositor.create_surface(&self.qh);
            let layer = self.layer_shell.create_layer_surface(
                &self.qh,
                surface,
                Layer::Overlay,
                Some("berth-hot-corner"),
                Some(&output),
            );
            layer.set_anchor(Anchor::TOP | side);
            layer.set_size(1, 1);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.commit();
            self.corners.push(CornerView {
                output,
                layer,
                buffer: None,
            });
        }
    }

    pub(super) fn corner_configured(&mut self, layer: &LayerSurface) {
        let Some(view) = self.corners.iter_mut().find(|view| view.owns(layer)) else {
            return;
        };
        if view.buffer.is_some() {
            return;
        }
        let Ok((buffer, pixel)) = self.pool.create_buffer(1, 1, 4, self.format) else {
            return;
        };
        pixel.fill(0);
        let surface = view.layer.wl_surface();
        if buffer.attach_to(surface).is_ok() {
            surface.damage_buffer(0, 0, 1, 1);
            surface.commit();
            view.buffer = Some(buffer);
        }
    }

    pub(super) fn corner_pointer(&mut self, event: &PointerEvent, left_at_once: bool) {
        let Some(view) = self.corners.iter().find(|view| view.is_on(&event.surface)) else {
            return;
        };
        match event.kind {
            PointerEventKind::Enter { .. } if !left_at_once && self.poke.arrived() => {
                let output = view.output.clone();
                self.toggle_overview(Some(output));
            }
            PointerEventKind::Leave { .. } => self.poke.left(),
            _ => {}
        }
    }
}
