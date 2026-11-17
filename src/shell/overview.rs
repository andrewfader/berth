use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::rc::Rc;

use cosmic_client_toolkit::sctk::compositor::Region;
use cosmic_client_toolkit::sctk::seat::keyboard::{KeyEvent, Keysym};
use cosmic_client_toolkit::sctk::seat::pointer::{PointerEvent, PointerEventKind};
use cosmic_client_toolkit::sctk::shell::WaylandSurface;
use cosmic_client_toolkit::sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerSurface};
use tiny_skia::Pixmap;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

use super::Berth;
use super::canvas::{Canvas, Medium};
use crate::dock::actions::Effect;
use crate::overview::layout::{Places, arrange};
use crate::overview::paint::{Card, Launcher, Scene, paint};
use crate::overview::{Overview, Pick};
use crate::switcher::thumbnail::worth_capturing;
use crate::windows::{AppId, WindowId};

const ICON: u32 = 128;
const TEXT: f32 = 13.0;
const SEARCH_TEXT: f32 = 16.0;
const HINT: &str = "Type to search";
const MOST_PIXELS: f32 = 4_500_000.0;

pub struct OverviewView {
    model: Overview,
    layer: LayerSurface,
    canvas: Canvas,
    blur: Option<ExtBackgroundEffectSurfaceV1>,
    screen: (u32, u32),
    titles: HashMap<WindowId, String>,
    names: HashMap<AppId, String>,
    icons: HashMap<Pick, Rc<Pixmap>>,
    texts: HashMap<(String, u32), Option<Rc<Pixmap>>>,
}

impl Drop for OverviewView {
    fn drop(&mut self) {
        if let Some(blur) = &self.blur {
            blur.destroy();
        }
    }
}

impl OverviewView {
    pub fn owns(&self, layer: &LayerSurface) -> bool {
        self.layer.wl_surface() == layer.wl_surface()
    }

    pub fn is_on(&self, surface: &WlSurface) -> bool {
        self.canvas.surface() == surface
    }

    pub fn invalidate(&mut self) {
        self.canvas.invalidate();
    }

    pub fn frame(&mut self, surface: &WlSurface) {
        if self.is_on(surface) {
            self.canvas.frame_arrived();
        }
    }

    pub fn rescale(&mut self, surface: &WlSurface, scale120: u32, fractional: bool) {
        if self.is_on(surface)
            && self.canvas.follows_fractional_scale() == fractional
            && self.canvas.rescale(scale120)
        {
            self.texts.clear();
        }
    }

    pub fn forget(&mut self, window: WindowId) {
        self.model.forget(window);
        self.canvas.invalidate();
    }

    fn places(&self) -> Places {
        let stage = (self.screen.0 as f32, self.screen.1 as f32);
        arrange(stage, self.model.windows().len(), self.model.apps().len())
    }

    fn detail(&self) -> f32 {
        let scale = self.canvas.scale();
        let pixels = self.screen.0 as f32 * self.screen.1 as f32 * scale * scale;
        (MOST_PIXELS / pixels.max(1.0)).sqrt().min(1.0)
    }
}

impl Berth {
    pub(super) fn toggle_overview(&mut self) {
        if self.overview.take().is_some() {
            return;
        }
        self.switcher = None;
        self.close_menu();

        let mut titles = HashMap::new();
        let windows: Vec<(WindowId, String)> = self
            .windows
            .most_recent_first()
            .iter()
            .map(|window| {
                titles.insert(window.id, window.title.clone());
                (
                    window.id,
                    format!("{} {}", window.title, self.catalog.name(&window.app)),
                )
            })
            .collect();
        let apps = self.catalog.launchable();
        let names = apps.iter().cloned().collect();
        let everything: Vec<WindowId> = windows.iter().map(|(id, _)| *id).collect();
        self.thumbnails.request(worth_capturing(
            &everything,
            0,
            self.windows.active().map(|window| window.id),
            |window| self.thumbnails.has(window),
        ));

        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(
            &self.qh,
            surface.clone(),
            Layer::Overlay,
            Some("berth-overview"),
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
            .filter(|_| self.config.overview.blur)
            .map(|manager| manager.get_background_effect(&surface, &self.qh, ()));

        self.overview = Some(OverviewView {
            model: Overview::open(windows, apps),
            layer,
            canvas: Canvas::on(surface, &self.extras, &self.qh),
            blur,
            screen: (0, 0),
            titles,
            names,
            icons: HashMap::new(),
            texts: HashMap::new(),
        });
        self.capture_next();
    }

    pub(super) fn overview_configured(&mut self, size: (u32, u32)) {
        if let Some(view) = &mut self.overview
            && size.0 > 0
            && size.1 > 0
        {
            view.screen = size;
            view.canvas.invalidate();
        }
    }

    pub(super) fn paint_overview(&mut self) {
        let Some(view) = &mut self.overview else {
            return;
        };
        if view.screen == (0, 0) || !view.canvas.wants_paint() {
            return;
        }
        let (detail, places) = (view.detail(), view.places());
        let density = view.canvas.scale() * detail;
        let (windows, apps) = (view.model.windows(), view.model.apps());

        for pick in view.model.picks() {
            let app = match &pick {
                Pick::Window(window) => self.windows.get(*window).map(|window| window.app.clone()),
                Pick::App(app) => Some(app.clone()),
            };
            if let (Some(app), Entry::Vacant(place)) = (app, view.icons.entry(pick)) {
                let (icon, name) = (self.catalog.icon(&app), self.catalog.name(&app));
                place.insert(self.icons.icon(icon, &name, ICON, &mut self.typesetter));
            }
        }
        let mut written = |text: &str, size: f32, widest: f32| {
            let key = (text.to_owned(), (size * density) as u32);
            view.texts
                .entry(key)
                .or_insert_with(|| {
                    self.typesetter
                        .line(text, size * density, widest * density)
                        .map(Rc::new)
                })
                .clone()
        };
        let cell_wide = places.windows.first().map_or(0.0, |cell| cell.width);
        let titles: Vec<Option<Rc<Pixmap>>> = windows
            .iter()
            .map(|window| written(view.titles.get(window)?, TEXT, cell_wide))
            .collect();
        let names: Vec<Option<Rc<Pixmap>>> = apps
            .iter()
            .map(|app| written(view.names.get(app)?, TEXT, 112.0))
            .collect();
        let query = view.model.query().to_owned();
        let search = written(if query.is_empty() { HINT } else { &query }, SEARCH_TEXT, 400.0);

        let cards: Vec<Card> = windows
            .iter()
            .zip(&titles)
            .filter_map(|(window, title)| {
                Some(Card {
                    thumbnail: self.thumbnails.shot(*window),
                    icon: view.icons.get(&Pick::Window(*window))?,
                    title: title.as_deref(),
                })
            })
            .collect();
        let launchers: Vec<Launcher> = apps
            .iter()
            .zip(&names)
            .filter_map(|(app, name)| {
                Some(Launcher {
                    icon: view.icons.get(&Pick::App(app.clone()))?,
                    name: name.as_deref(),
                })
            })
            .collect();

        if let Some(blur) = &view.blur
            && let Ok(region) = Region::new(&self.compositor)
        {
            region.add(0, 0, view.screen.0 as i32, view.screen.1 as i32);
            blur.set_blur_region(Some(region.wl_region()));
        }
        let scene = Scene {
            places: &places,
            windows: &cards,
            apps: &launchers,
            selected: view.model.selected().map(|_| view.model.selected_index()),
            search: search.as_deref(),
            dimness: self.config.overview.opacity,
            scale: density,
        };
        view.canvas.present(
            Medium {
                pool: &mut self.pool,
                format: self.format,
                qh: &self.qh,
            },
            view.screen,
            detail,
            false,
            |canvas, scale| paint(canvas, &Scene { scale, ..scene }),
        );
    }

    pub(super) fn overview_pointer(&mut self, event: &PointerEvent) {
        let Some(view) = &mut self.overview else {
            return;
        };
        let pointed = view
            .places()
            .pick_at(event.position.0 as f32, event.position.1 as f32);
        match event.kind {
            PointerEventKind::Motion { .. } | PointerEventKind::Enter { .. } => {
                if let Some(index) = pointed.filter(|index| *index != view.model.selected_index()) {
                    view.model.select(index);
                    view.canvas.invalidate();
                }
            }
            PointerEventKind::Release { .. } => {
                let chosen = pointed.and_then(|index| view.model.picks().into_iter().nth(index));
                self.leave_overview(chosen);
            }
            _ => {}
        }
    }

    pub(super) fn overview_key(&mut self, event: &KeyEvent) {
        let Some(view) = &mut self.overview else {
            return;
        };
        let columns = view.places().columns as isize;
        view.canvas.invalidate();
        match event.keysym {
            Keysym::Escape => self.leave_overview(None),
            Keysym::Return | Keysym::KP_Enter => {
                let chosen = view.model.selected();
                self.leave_overview(chosen);
            }
            Keysym::Right | Keysym::Tab => view.model.step(1),
            Keysym::Left | Keysym::ISO_Left_Tab => view.model.step(-1),
            Keysym::Down => view.model.step(columns),
            Keysym::Up => view.model.step(-columns),
            Keysym::BackSpace => view.model.erased(),
            _ => {
                if let Some(text) = &event.utf8 {
                    view.model.typed(text);
                }
            }
        }
    }

    fn leave_overview(&mut self, chosen: Option<Pick>) {
        self.overview = None;
        match chosen {
            Some(Pick::Window(window)) => self.perform(Effect::Activate(window)),
            Some(Pick::App(app)) => self.perform(Effect::Launch(app)),
            None => {}
        }
    }
}
