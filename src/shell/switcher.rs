use std::rc::Rc;
use std::time::Duration;

use cosmic_client_toolkit::sctk::compositor::Region;
use cosmic_client_toolkit::sctk::seat::keyboard::Keysym;
use cosmic_client_toolkit::sctk::seat::pointer::{PointerEvent, PointerEventKind};
use cosmic_client_toolkit::sctk::shell::WaylandSurface;
use cosmic_client_toolkit::sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerSurface};
use tiny_skia::{Color, Pixmap};
use wayland_client::protocol::wl_subsurface::WlSubsurface;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

use super::Berth;
use super::canvas::{Canvas, Medium};
use crate::animation::Animated;
use crate::dock::actions::Effect;
use crate::paint::Finish;
use crate::switcher::carousel::{self, Cover};
use crate::switcher::paint::{Card, Scene, paint, picture_frame};
use crate::switcher::session::{Direction, Session};
use crate::windows::WindowId;

const GLIDE: Duration = Duration::from_millis(190);
const HESITATION: Duration = Duration::from_millis(110);
const BACKDROP_DETAIL: f32 = 0.05;
const MOTION_DETAIL: f32 = 0.5;
const DENSE: f32 = 1.5;
const ICON: u32 = 128;

struct Stage {
    canvas: Canvas,
    subsurface: WlSubsurface,
    size: (u32, u32),
}

impl Drop for Stage {
    fn drop(&mut self) {
        self.subsurface.destroy();
        self.canvas.surface().destroy();
    }
}

pub struct SwitcherView {
    session: Session,
    stage: Option<Stage>,
    backdrop: Canvas,
    blur: Option<ExtBackgroundEffectSurfaceV1>,
    layer: LayerSurface,
    screen: (u32, u32),
    icons: Vec<Rc<Pixmap>>,
    title: Option<(WindowId, Rc<Pixmap>)>,
    focus: Animated,
    opened: Duration,
    lit: bool,
    focused: bool,
}

impl SwitcherView {
    pub fn owns(&self, layer: &LayerSurface) -> bool {
        self.layer.wl_surface() == layer.wl_surface()
    }

    pub fn invalidate(&mut self) {
        self.title = None;
        if let Some(stage) = &mut self.stage {
            stage.canvas.invalidate();
        }
    }

    pub fn frame(&mut self, surface: &WlSurface) {
        if let Some(stage) = self
            .stage
            .as_mut()
            .filter(|stage| stage.canvas.surface() == surface)
        {
            stage.canvas.frame_arrived();
        }
    }

    pub fn rescale(&mut self, surface: &WlSurface, scale120: u32, fractional: bool) {
        for canvas in [
            Some(&mut self.backdrop),
            self.stage.as_mut().map(|stage| &mut stage.canvas),
        ]
        .into_iter()
        .flatten()
        {
            if canvas.surface() == surface && canvas.follows_fractional_scale() == fractional {
                canvas.rescale(scale120);
            }
        }
        self.title = None;
    }

    pub fn without(self, window: WindowId) -> Option<Self> {
        let at = self
            .session
            .candidates()
            .iter()
            .position(|candidate| *candidate == window);
        let mut view = self;
        if let Some(at) = at {
            view.icons.remove(at);
        }
        let session = view.session.clone().forget(window)?;
        view.focus.jump_to(session.selected_index() as f32);
        view.session = session;
        view.invalidate();
        Some(view)
    }

    fn covers(&self, focus: f32) -> Vec<Cover> {
        let stage = self
            .stage
            .as_ref()
            .map_or((0.0, 0.0), |stage| (stage.size.0 as f32, stage.size.1 as f32));
        carousel::arrange(self.session.candidates().len(), focus, stage)
    }
}

impl Berth {
    pub(super) fn switch(&mut self, direction: Direction) {
        let now = self.now();
        if let Some(view) = &mut self.switcher {
            view.session.step(direction);
            view.focus
                .head_for(view.session.selected_index() as f32, GLIDE, now);
            view.invalidate();
            return;
        }
        let Some(session) = Session::begin(&self.windows, direction, self.config.switcher.windows) else {
            return;
        };

        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(
            &self.qh,
            surface.clone(),
            Layer::Overlay,
            Some("berth-switcher"),
            None,
        );
        layer.set_anchor(Anchor::all());
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.commit();

        let blur = self
            .extras
            .blur
            .as_ref()
            .filter(|_| self.config.switcher.blur)
            .map(|manager| manager.get_background_effect(&surface, &self.qh, ()));
        let icons = session
            .candidates()
            .iter()
            .filter_map(|id| self.windows.get(*id))
            .map(|window| {
                let (icon, name) = (self.catalog.icon(&window.app), self.catalog.name(&window.app));
                self.icons.icon(icon, &name, ICON, &mut self.typesetter)
            })
            .collect();

        self.thumbnails
            .request(session.candidates(), session.selected_index());
        self.switcher = Some(SwitcherView {
            focus: Animated::resting_at(session.selected_index() as f32),
            session,
            stage: None,
            backdrop: Canvas::on(surface, &self.extras, &self.qh),
            blur,
            layer,
            screen: (0, 0),
            icons,
            title: None,
            opened: now,
            lit: false,
            focused: false,
        });
        self.capture_next();
    }

    pub(super) fn switcher_configured(&mut self, size: (u32, u32)) {
        let Some(view) = &mut self.switcher else { return };
        if size.0 == 0 || size.1 == 0 || size == view.screen {
            return;
        }
        view.screen = size;
        view.backdrop.invalidate();

        let stage = carousel::stage_on(size);
        let parent = view.backdrop.surface().clone();
        let (subsurface, surface) = self.subcompositor.create_subsurface(parent, &self.qh);
        subsurface.set_position(stage.x as i32, stage.y as i32);
        subsurface.set_desync();
        view.stage = Some(Stage {
            canvas: Canvas::on(surface, &self.extras, &self.qh),
            subsurface,
            size: (stage.width as u32, stage.height as u32),
        });
    }

    pub(super) fn switcher_deadline(&self) -> Option<Duration> {
        self.switcher
            .as_ref()
            .filter(|view| !view.lit)
            .map(|view| view.opened + HESITATION)
    }

    pub(super) fn settle_switcher(&mut self) {
        let now = self.now();
        if let Some(view) = &mut self.switcher
            && !view.lit
            && now >= view.opened + HESITATION
        {
            view.lit = true;
            view.backdrop.invalidate();
            view.invalidate();
        }
    }

    pub(super) fn paint_switcher(&mut self) {
        let now = self.now();
        let Some(view) = &mut self.switcher else { return };
        if view.screen == (0, 0) {
            return;
        }

        if view.backdrop.wants_paint() {
            let shade = if view.lit {
                self.config.switcher.opacity
            } else {
                0.0
            };
            if let Some(blur) = view.blur.as_ref().filter(|_| view.lit)
                && let Ok(region) = Region::new(&self.compositor)
            {
                region.add(0, 0, view.screen.0 as i32, view.screen.1 as i32);
                blur.set_blur_region(Some(region.wl_region()));
            }
            view.backdrop.present(
                Medium {
                    pool: &mut self.pool,
                    format: self.format,
                    qh: &self.qh,
                },
                view.screen,
                BACKDROP_DETAIL,
                false,
                |canvas, _| {
                    canvas.fill(Color::from_rgba(0.0, 0.0, 0.0, shade).unwrap_or(Color::TRANSPARENT));
                },
            );
        }

        if !view.lit
            || !view
                .stage
                .as_ref()
                .is_some_and(|stage| stage.canvas.wants_paint())
        {
            return;
        }
        let focus = view.focus.value(now);
        let covers = view.covers(focus);
        let selected = view.session.selected();
        let scale = view.stage.as_ref().map_or(1.0, |stage| stage.canvas.scale());
        if view.title.as_ref().is_none_or(|(window, _)| *window != selected) {
            let widest = view.stage.as_ref().map_or(0.0, |stage| stage.size.0 as f32 * 0.7);
            view.title = self
                .windows
                .get(selected)
                .and_then(|window| self.typesetter.line(&window.title, 17.0 * scale, widest * scale))
                .map(|title| (selected, Rc::new(title)));
        }
        let cards: Vec<Card> = view
            .session
            .candidates()
            .iter()
            .zip(&view.icons)
            .map(|(window, icon)| Card {
                thumbnail: self.thumbnails.shot(*window),
                icon,
            })
            .collect();
        let animating = !view.focus.is_settled(now);
        let scene = Scene {
            covers: &covers,
            cards: &cards,
            selected: view.session.selected_index(),
            title: view.title.as_ref().map(|(_, title)| title.as_ref()),
            finish: if animating { Finish::Draft } else { Finish::Fine },
            scale,
        };
        let detail = if animating && scale >= DENSE {
            MOTION_DETAIL
        } else {
            1.0
        };
        if let Some(stage) = &mut view.stage {
            stage.canvas.present(
                Medium {
                    pool: &mut self.pool,
                    format: self.format,
                    qh: &self.qh,
                },
                stage.size,
                detail,
                animating,
                |canvas, scale| paint(canvas, &Scene { scale, ..scene }),
            );
        }
    }

    pub(super) fn switcher_pointer(&mut self, event: &PointerEvent) {
        let Some(view) = &mut self.switcher else { return };
        if !matches!(event.kind, PointerEventKind::Release { .. }) {
            return;
        }
        let on_stage = view
            .stage
            .as_ref()
            .is_some_and(|stage| stage.canvas.surface() == &event.surface);
        let (x, y) = (event.position.0 as f32, event.position.1 as f32);
        let picked = view
            .covers(view.focus.target())
            .into_iter()
            .rev()
            .filter(|_| on_stage)
            .find(|cover| {
                let card = Card {
                    thumbnail: self.thumbnails.shot(view.session.candidates()[cover.index]),
                    icon: &view.icons[cover.index],
                };
                picture_frame(&card, cover.frame).contains(x, y)
            })
            .map(|cover| view.session.candidates()[cover.index]);
        self.finish_switch(picked);
    }

    pub(super) fn switcher_focused(&mut self) {
        if let Some(view) = &mut self.switcher {
            view.focused = true;
        }
    }

    pub(super) fn switcher_modifiers(&mut self) {
        let held = self.modifiers.alt || self.modifiers.logo || self.modifiers.ctrl;
        if let Some(view) = &self.switcher
            && view.focused
            && !held
        {
            self.finish_switch(Some(view.session.selected()));
        }
    }

    pub(super) fn switcher_key(&mut self, key: Keysym) {
        let Some(view) = &self.switcher else { return };
        match key {
            Keysym::Escape => self.finish_switch(None),
            Keysym::Return | Keysym::KP_Enter | Keysym::space => {
                self.finish_switch(Some(view.session.selected()))
            }
            Keysym::ISO_Left_Tab | Keysym::Left | Keysym::Up => self.switch(Direction::Previous),
            Keysym::Tab if self.modifiers.shift => self.switch(Direction::Previous),
            Keysym::Tab | Keysym::Right | Keysym::Down => self.switch(Direction::Next),
            _ => {}
        }
    }

    fn finish_switch(&mut self, chosen: Option<WindowId>) {
        self.switcher = None;
        if let Some(window) = chosen {
            self.perform(Effect::Activate(window));
        }
    }
}
