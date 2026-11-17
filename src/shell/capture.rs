use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use cosmic_client_toolkit::delegate_screencopy;
use cosmic_client_toolkit::screencopy::{
    CaptureFrame, CaptureOptions, CaptureSession, CaptureSource, FailureReason, Formats, Frame,
    ScreencopyFrameData, ScreencopyHandler, ScreencopySessionData, ScreencopyState,
};
use cosmic_client_toolkit::sctk::shm::raw::RawPool;
use tiny_skia::Pixmap;
use wayland_client::protocol::{wl_buffer::WlBuffer, wl_shm};
use wayland_client::{Connection, QueueHandle, WEnum};

use super::Berth;
use crate::switcher::thumbnail::{Capture, Channels, LONGEST_SIDE, thumbnail};
use crate::windows::WindowId;

#[derive(Default)]
pub struct Thumbnails {
    shots: HashMap<WindowId, Rc<Pixmap>>,
    waiting: VecDeque<WindowId>,
    shooting: Option<Shooting>,
}

struct Shooting {
    window: WindowId,
    session: CaptureSession,
    film: Option<Film>,
}

struct Film {
    pool: RawPool,
    buffer: WlBuffer,
    size: (u32, u32),
    channels: Channels,
    opaque: bool,
}

impl Thumbnails {
    pub fn shot(&self, window: WindowId) -> Option<&Pixmap> {
        self.shots.get(&window).map(Rc::as_ref)
    }

    pub fn has(&self, window: WindowId) -> bool {
        self.shots.contains_key(&window)
    }

    pub fn request(&mut self, windows: Vec<WindowId>) {
        self.waiting = windows.into();
    }

    pub fn refresh(&mut self, window: WindowId) {
        if !self.waiting.contains(&window) {
            self.waiting.push_back(window);
        }
    }

    pub fn forget(&mut self, window: WindowId) {
        self.shots.remove(&window);
        self.waiting.retain(|waiting| *waiting != window);
    }
}

fn layout_of(formats: &[wl_shm::Format]) -> Option<(wl_shm::Format, Channels, bool)> {
    [
        (wl_shm::Format::Abgr8888, Channels::Rgba, false),
        (wl_shm::Format::Xbgr8888, Channels::Rgba, true),
        (wl_shm::Format::Argb8888, Channels::Bgra, false),
        (wl_shm::Format::Xrgb8888, Channels::Bgra, true),
    ]
    .into_iter()
    .find(|(format, _, _)| formats.contains(format))
}

impl Berth {
    pub(super) fn capture_next(&mut self) {
        while self.thumbnails.shooting.is_none() {
            let Some(window) = self.thumbnails.waiting.pop_front() else {
                return;
            };
            let Some(handle) = self.handle_of(window) else {
                continue;
            };
            let session = self.screencopy.capturer().create_session(
                &CaptureSource::Toplevel(handle.clone()),
                CaptureOptions::empty(),
                &self.qh,
                ScreencopySessionData::default(),
            );
            match session {
                Ok(session) => {
                    self.thumbnails.shooting = Some(Shooting {
                        window,
                        session,
                        film: None,
                    })
                }
                Err(error) => log::warn!("window thumbnails are unavailable: {error}"),
            }
        }
    }

    fn capture_finished(&mut self) -> Option<(WindowId, Film)> {
        let shooting = self.thumbnails.shooting.take();
        self.capture_next();
        let shooting = shooting?;
        let film = shooting.film?;
        film.buffer.destroy();
        Some((shooting.window, film))
    }
}

impl ScreencopyHandler for Berth {
    fn screencopy_state(&mut self) -> &mut ScreencopyState {
        &mut self.screencopy
    }

    fn init_done(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        session: &CaptureSession,
        formats: &Formats,
    ) {
        let Some(shooting) = self
            .thumbnails
            .shooting
            .as_mut()
            .filter(|shooting| shooting.session == *session)
        else {
            return;
        };
        if shooting.film.is_some() {
            return;
        }
        let (wide, tall) = formats.buffer_size;
        let prepared = layout_of(&formats.shm_formats)
            .filter(|_| wide > 0 && tall > 0)
            .and_then(|layout| {
                let pool = RawPool::new(wide as usize * tall as usize * 4, &self.shm).ok()?;
                Some((layout, pool))
            });
        let Some(((format, channels, opaque), mut pool)) = prepared else {
            self.capture_finished();
            return;
        };
        let buffer = pool.create_buffer(0, wide as i32, tall as i32, wide as i32 * 4, format, (), qh);
        session.capture(&buffer, &[], qh, ScreencopyFrameData::default());
        shooting.film = Some(Film {
            pool,
            buffer,
            size: (wide, tall),
            channels,
            opaque,
        });
    }

    fn stopped(&mut self, _: &Connection, _: &QueueHandle<Self>, session: &CaptureSession) {
        if self
            .thumbnails
            .shooting
            .as_ref()
            .is_some_and(|shooting| shooting.session == *session)
        {
            self.capture_finished();
        }
    }

    fn ready(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &CaptureFrame, _: Frame) {
        let Some((window, mut film)) = self.capture_finished() else {
            return;
        };
        let capture = Capture {
            bytes: film.pool.mmap(),
            size: film.size,
            channels: film.channels,
            opaque: film.opaque,
        };
        if let Some(shot) = thumbnail(&capture, LONGEST_SIDE) {
            self.thumbnails.shots.insert(window, Rc::new(shot));
            if let Some(view) = &mut self.switcher {
                view.invalidate();
            }
        }
    }

    fn failed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &CaptureFrame,
        reason: WEnum<FailureReason>,
    ) {
        log::debug!("a window could not be captured: {reason:?}");
        self.capture_finished();
    }
}

delegate_screencopy!(Berth);
