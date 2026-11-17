use cosmic_client_toolkit::sctk::shm::slot::SlotPool;
use tiny_skia::PixmapMut;
use wayland_client::QueueHandle;
use wayland_client::protocol::{wl_shm, wl_surface::WlSurface};
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1;
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;

use super::{Berth, Extras};

pub const WHOLE: u32 = 120;

pub struct Medium<'a> {
    pub pool: &'a mut SlotPool,
    pub format: wl_shm::Format,
    pub qh: &'a QueueHandle<Berth>,
}

pub struct Canvas {
    surface: WlSurface,
    viewport: Option<WpViewport>,
    fractional: Option<WpFractionalScaleV1>,
    scale120: u32,
    awaiting_frame: bool,
    stale: bool,
}

impl Canvas {
    pub fn on(surface: WlSurface, extras: &Extras, qh: &QueueHandle<Berth>) -> Self {
        let viewport = extras
            .viewporter
            .as_ref()
            .map(|viewporter| viewporter.get_viewport(&surface, qh, ()));
        let fractional = viewport
            .as_ref()
            .and(extras.fractional.as_ref())
            .map(|manager| manager.get_fractional_scale(&surface, qh, surface.clone()));
        Self {
            surface,
            viewport,
            fractional,
            scale120: WHOLE,
            awaiting_frame: false,
            stale: true,
        }
    }

    pub fn surface(&self) -> &WlSurface {
        &self.surface
    }

    pub fn scale(&self) -> f32 {
        self.scale120 as f32 / WHOLE as f32
    }

    pub fn follows_fractional_scale(&self) -> bool {
        self.fractional.is_some()
    }

    pub fn rescale(&mut self, scale120: u32) -> bool {
        let changed = scale120 != self.scale120;
        self.scale120 = scale120;
        self.stale |= changed;
        changed
    }

    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    pub fn frame_arrived(&mut self) {
        self.awaiting_frame = false;
    }

    pub fn wants_paint(&self) -> bool {
        self.stale && !self.awaiting_frame
    }

    pub fn present(
        &mut self,
        medium: Medium,
        size: (u32, u32),
        detail: f32,
        animating: bool,
        draw: impl FnOnce(&mut PixmapMut, f32),
    ) {
        let Medium { pool, format, qh } = medium;
        let density = if self.viewport.is_some() {
            self.scale() * detail
        } else {
            self.scale().round()
        };
        let wide = ((size.0 as f32 * density).round() as i32).max(1);
        let tall = ((size.1 as f32 * density).round() as i32).max(1);
        let Ok((buffer, bytes)) = pool.create_buffer(wide, tall, wide * 4, format) else {
            log::error!("no memory for a {wide}x{tall} buffer");
            return;
        };
        bytes.fill(0);
        if let Some(mut pixmap) = PixmapMut::from_bytes(bytes, wide as u32, tall as u32) {
            draw(&mut pixmap, density);
        }
        if format == wl_shm::Format::Argb8888 {
            bytes
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|pixel| pixel.swap(0, 2));
        }
        if buffer.attach_to(&self.surface).is_err() {
            return;
        }
        self.surface.damage_buffer(0, 0, wide, tall);
        match &self.viewport {
            Some(viewport) => viewport.set_destination(size.0 as i32, size.1 as i32),
            None => self.surface.set_buffer_scale(density as i32),
        }
        if animating {
            self.surface.frame(qh, self.surface.clone());
        }
        self.awaiting_frame = animating;
        self.stale = animating;
        self.surface.commit();
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        if let Some(fractional) = &self.fractional {
            fractional.destroy();
        }
        if let Some(viewport) = &self.viewport {
            viewport.destroy();
        }
    }
}
