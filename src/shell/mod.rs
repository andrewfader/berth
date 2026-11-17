mod canvas;
mod capture;
mod dock;
mod menu;
mod switcher;

use std::collections::HashMap;
use std::fs;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use calloop::generic::Generic;
use calloop::signals::{Signal, Signals};
use calloop::{EventLoop, Interest, Mode, PostAction};
use calloop_wayland_source::WaylandSource;
use cosmic_client_toolkit::cosmic_protocols::overlap_notify::v1::client::zcosmic_overlap_notify_v1::ZcosmicOverlapNotifyV1;
use cosmic_client_toolkit::cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1::State as ToplevelState;
use cosmic_client_toolkit::cosmic_protocols::toplevel_management::v1::client::zcosmic_toplevel_manager_v1;
use cosmic_client_toolkit::screencopy::ScreencopyState;
use cosmic_client_toolkit::sctk::compositor::{CompositorHandler, CompositorState};
use cosmic_client_toolkit::sctk::globals::GlobalData;
use cosmic_client_toolkit::sctk::output::{OutputHandler, OutputState};
use cosmic_client_toolkit::sctk::registry::{ProvidesRegistryState, RegistryState};
use cosmic_client_toolkit::sctk::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers,
};
use cosmic_client_toolkit::sctk::seat::pointer::{PointerEvent, PointerHandler};
use cosmic_client_toolkit::sctk::seat::{Capability, SeatHandler, SeatState};
use cosmic_client_toolkit::sctk::shell::wlr_layer::{
    LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use cosmic_client_toolkit::sctk::shell::xdg::XdgShell;
use cosmic_client_toolkit::sctk::shell::xdg::popup::{Popup, PopupConfigure, PopupHandler};
use cosmic_client_toolkit::sctk::shm::slot::SlotPool;
use cosmic_client_toolkit::sctk::shm::{Shm, ShmHandler};
use cosmic_client_toolkit::sctk::subcompositor::SubcompositorState;
use cosmic_client_toolkit::sctk::{
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm, delegate_subcompositor, delegate_xdg_popup,
    registry_handlers,
};
use cosmic_client_toolkit::toplevel_info::{ToplevelInfoHandler, ToplevelInfoState};
use cosmic_client_toolkit::toplevel_management::{ToplevelManagerHandler, ToplevelManagerState};
use cosmic_client_toolkit::workspace::{WorkspaceHandler, WorkspaceState};
use cosmic_client_toolkit::{delegate_toplevel_info, delegate_toplevel_manager, delegate_workspace};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{
    wl_buffer::WlBuffer, wl_keyboard::WlKeyboard, wl_output, wl_pointer::WlPointer, wl_seat::WlSeat, wl_shm,
    wl_surface::WlSurface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum, delegate_dispatch, delegate_noop};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1;
use wayland_protocols::ext::workspace::v1::client::ext_workspace_handle_v1::State as WorkspaceVisibility;
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
use wayland_protocols::wp::viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter};
use wayland_protocols::xdg::decoration::zv1::client::zxdg_decoration_manager_v1::ZxdgDecorationManagerV1;
use wayland_protocols::xdg::shell::client::xdg_wm_base::XdgWmBase;

use crate::apps::{self, Catalog};
use crate::config::{Config, ConfigError, config_home};
use crate::cosmic;
use crate::dock::actions::Effect;
use crate::icons::Icons;
use crate::ipc::{self, Request};
use crate::pins::{self, Pins};
use crate::text::Typesetter;
use crate::windows::{self, AppId, Window, WindowId, Windows};
use capture::Thumbnails;
use dock::DockView;
use menu::MenuView;
use switcher::SwitcherView;

pub struct Extras {
    pub viewporter: Option<WpViewporter>,
    pub fractional: Option<WpFractionalScaleManagerV1>,
    pub blur: Option<ExtBackgroundEffectManagerV1>,
    pub overlap: Option<ZcosmicOverlapNotifyV1>,
}

pub struct Berth {
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    compositor: CompositorState,
    subcompositor: SubcompositorState,
    shm: Shm,
    pool: SlotPool,
    format: wl_shm::Format,
    layer_shell: LayerShell,
    xdg_shell: XdgShell,
    toplevels: ToplevelInfoState,
    workspaces: WorkspaceState,
    manager: Option<ToplevelManagerState>,
    screencopy: ScreencopyState,
    extras: Extras,
    seat: Option<WlSeat>,
    pointer: Option<WlPointer>,
    keyboard: Option<WlKeyboard>,
    modifiers: Modifiers,
    qh: QueueHandle<Berth>,
    started: Instant,
    config: Config,
    catalog: Catalog,
    identities: HashMap<String, AppId>,
    icons: Icons,
    typesetter: Typesetter,
    windows: Windows,
    ids: HashMap<ExtForeignToplevelHandleV1, WindowId>,
    opened: u32,
    pins: Pins,
    docks: Vec<DockView>,
    menu: Option<MenuView>,
    launching: HashMap<AppId, Duration>,
    switcher: Option<SwitcherView>,
    thumbnails: Thumbnails,
    leaving: bool,
}

pub fn run(config: Config) -> anyhow::Result<()> {
    let connection = Connection::connect_to_env().context("no Wayland compositor to connect to")?;
    let (globals, queue) = registry_queue_init::<Berth>(&connection)?;
    let qh = queue.handle();
    let mut event_loop: EventLoop<Berth> = EventLoop::try_new()?;
    WaylandSource::new(connection.clone(), queue)
        .insert(event_loop.handle())
        .map_err(|error| anyhow!("{error}"))?;

    let socket = ipc::socket_path();
    let listener = ipc::listen(&socket)?;
    event_loop
        .handle()
        .insert_source(
            Signals::new(&[Signal::SIGTERM, Signal::SIGINT])?,
            |_, _, berth| berth.leaving = true,
        )
        .map_err(|error| anyhow!("{error}"))?;
    event_loop
        .handle()
        .insert_source(
            Generic::new(listener, Interest::READ, Mode::Level),
            |_, listener, berth| {
                ipc::answer_pending(listener, |request| berth.answer(request));
                Ok(PostAction::Continue)
            },
        )
        .map_err(|error| anyhow!("{error}"))?;

    let registry = RegistryState::new(&globals);
    let compositor =
        CompositorState::bind(&globals, &qh).context("the compositor offers no wl_compositor")?;
    let subcompositor = SubcompositorState::bind(compositor.wl_compositor().clone(), &globals, &qh)
        .context("the compositor offers no wl_subcompositor")?;
    let shm = Shm::bind(&globals, &qh).context("the compositor offers no wl_shm")?;
    let layer_shell =
        LayerShell::bind(&globals, &qh).context("the compositor does not support layer-shell")?;
    let toplevels = ToplevelInfoState::try_new(&registry, &qh)
        .context("the compositor does not list windows (ext-foreign-toplevel-list)")?;
    let extras = Extras {
        viewporter: globals.bind(&qh, 1..=1, ()).ok(),
        fractional: globals.bind(&qh, 1..=1, ()).ok(),
        blur: globals.bind(&qh, 1..=1, ()).ok(),
        overlap: globals.bind(&qh, 1..=1, ()).ok(),
    };
    let format = if shm.formats().contains(&wl_shm::Format::Abgr8888) {
        wl_shm::Format::Abgr8888
    } else {
        wl_shm::Format::Argb8888
    };
    let catalog = Catalog::installed();
    let xdg_shell = XdgShell::bind(&globals, &qh).context("the compositor does not support xdg-shell")?;
    let pins = pinned_apps(&config, &catalog);

    let mut berth = Berth {
        outputs: OutputState::new(&globals, &qh),
        seats: SeatState::new(&globals, &qh),
        pool: SlotPool::new(1 << 20, &shm)?,
        manager: ToplevelManagerState::try_new(&registry, &qh),
        workspaces: WorkspaceState::new(&registry, &qh),
        screencopy: ScreencopyState::new(&globals, &qh),
        icons: themed_icons(),
        registry,
        compositor,
        subcompositor,
        shm,
        format,
        layer_shell,
        xdg_shell,
        toplevels,
        extras,
        seat: None,
        pointer: None,
        keyboard: None,
        modifiers: Modifiers::default(),
        qh,
        started: Instant::now(),
        config,
        catalog,
        identities: HashMap::new(),
        typesetter: Typesetter::default(),
        windows: Windows::default(),
        ids: HashMap::new(),
        opened: 0,
        pins,
        docks: Vec::new(),
        menu: None,
        launching: HashMap::new(),
        switcher: None,
        thumbnails: Thumbnails::default(),
        leaving: false,
    };

    while !berth.leaving {
        let wait = berth
            .next_deadline()
            .map(|deadline| deadline.saturating_sub(berth.now()));
        event_loop.dispatch(wait, &mut berth)?;
        berth.tick();
    }
    fs::remove_file(socket)?;
    Ok(())
}

fn pinned_apps(config: &Config, catalog: &Catalog) -> Pins {
    match &config.dock.pinned {
        Some(fixed) => Pins::fixed(catalog.pinned(fixed)),
        None => {
            let store = pins::store();
            let wanted = Pins::remembered(&store)
                .or_else(|| cosmic::setting(&config_home(), cosmic::FAVORITES))
                .unwrap_or_default();
            Pins::kept_in(store, catalog.pinned(&wanted))
        }
    }
}

fn themed_icons() -> Icons {
    Icons::themed(
        cosmic::setting(&config_home(), cosmic::ICON_THEME)
            .or_else(freedesktop_icons::default_theme_gtk)
            .unwrap_or_else(|| "hicolor".to_owned()),
    )
}

impl Berth {
    fn now(&self) -> Duration {
        self.started.elapsed()
    }

    fn next_deadline(&self) -> Option<Duration> {
        [self.dock_deadline(), self.switcher_deadline()]
            .into_iter()
            .flatten()
            .min()
    }

    fn tick(&mut self) {
        self.settle_docks();
        self.paint_docks();
        self.paint_menu();
        self.settle_switcher();
        self.paint_switcher();
    }

    fn answer(&mut self, request: Request) -> String {
        match request {
            Request::Switch(direction) => {
                self.switch(direction);
                "ok\n".to_owned()
            }
            Request::Windows => windows::listing(&self.windows),
            Request::Reload => match self.reload() {
                Ok(()) => "ok\n".to_owned(),
                Err(problem) => format!("error: {problem}\n"),
            },
        }
    }

    fn reload(&mut self) -> Result<(), ConfigError> {
        self.config = Config::load(&Config::path())?;
        self.catalog = Catalog::installed();
        self.identities.clear();
        self.pins = pinned_apps(&self.config, &self.catalog);
        self.icons = themed_icons();
        self.switcher = None;
        self.close_docks(|_| true);
        for output in self.outputs.outputs().collect::<Vec<_>>() {
            self.open_dock(output);
        }
        Ok(())
    }

    fn perform(&mut self, effect: Effect) {
        match effect {
            Effect::Launch(app) => match self.catalog.command(&app) {
                Some(command) => match apps::launch(&command) {
                    Ok(()) => {
                        self.launching.insert(app, self.now());
                        self.refresh_dock_items();
                    }
                    Err(error) => log::error!("could not start {}: {error}", app.as_str()),
                },
                None => log::warn!("{} has no launcher to start it with", app.as_str()),
            },
            Effect::Pin(app) => self.repin(|pins| pins.pin(app)),
            Effect::Unpin(app) => self.repin(|pins| pins.unpin(&app)),
            Effect::Activate(window) => {
                self.manage(window, |manager, toplevel, seat| manager.activate(toplevel, seat))
            }
            Effect::Minimize(window) => {
                self.manage(window, |manager, toplevel, _| manager.set_minimized(toplevel))
            }
            Effect::Close(window) => self.manage(window, |manager, toplevel, _| manager.close(toplevel)),
        }
    }

    fn repin(&mut self, change: impl FnOnce(&mut Pins) -> std::io::Result<()>) {
        if let Err(error) = change(&mut self.pins) {
            log::error!("could not save the pinned apps: {error}");
        }
        self.refresh_dock_items();
    }

    fn manage(
        &self,
        window: WindowId,
        request: impl FnOnce(
            &zcosmic_toplevel_manager_v1::ZcosmicToplevelManagerV1,
            &cosmic_client_toolkit::cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
            &WlSeat,
        ),
    ) {
        let toplevel = self
            .handle_of(window)
            .and_then(|handle| self.toplevels.info(handle))
            .and_then(|info| info.cosmic_toplevel.as_ref());
        match (&self.manager, toplevel, &self.seat) {
            (Some(manager), Some(toplevel), Some(seat)) => request(&manager.manager, toplevel, seat),
            _ => log::warn!("the compositor does not let berth manage this window"),
        }
    }

    fn handle_of(&self, window: WindowId) -> Option<&ExtForeignToplevelHandleV1> {
        self.ids
            .iter()
            .find(|(_, id)| **id == window)
            .map(|(handle, _)| handle)
    }

    fn sync_window(&mut self, handle: &ExtForeignToplevelHandleV1) {
        let Some(info) = self.toplevels.info(handle) else {
            return;
        };
        let id = *self.ids.entry(handle.clone()).or_insert_with(|| {
            self.opened += 1;
            WindowId(self.opened)
        });
        let app = match self.identities.get(&info.app_id) {
            Some(app) => app.clone(),
            None => {
                let app = self.catalog.identify(&info.app_id);
                self.identities.insert(info.app_id.clone(), app.clone());
                app
            }
        };
        let on_a_workspace_in_view = info.workspace.iter().any(|workspace| {
            self.workspaces
                .workspace_info(workspace)
                .is_some_and(|known| known.state.contains(WorkspaceVisibility::Active))
        });
        self.launching.remove(&app);
        self.windows.report(Window {
            id,
            app,
            title: info.title.clone(),
            active: info.state.contains(&ToplevelState::Activated),
            minimized: info.state.contains(&ToplevelState::Minimized),
            monitors: info
                .output
                .iter()
                .filter_map(|output| self.outputs.info(output)?.name)
                .collect(),
            here: info.workspace.is_empty()
                || on_a_workspace_in_view
                || info.state.contains(&ToplevelState::Sticky),
        });
        self.windows_changed();
    }

    fn windows_changed(&mut self) {
        self.refresh_dock_items();
        if let Some(view) = &mut self.switcher {
            view.invalidate();
        }
    }

    fn rescale(&mut self, surface: &WlSurface, scale120: u32, fractional: bool) {
        self.rescale_dock(surface, scale120, fractional);
        self.rescale_menu(surface, scale120, fractional);
        if let Some(view) = &mut self.switcher {
            view.rescale(surface, scale120, fractional);
        }
    }
}

impl ToplevelInfoHandler for Berth {
    fn toplevel_info_state(&mut self) -> &mut ToplevelInfoState {
        &mut self.toplevels
    }

    fn new_toplevel(&mut self, _: &Connection, _: &QueueHandle<Self>, toplevel: &ExtForeignToplevelHandleV1) {
        self.sync_window(toplevel);
    }

    fn update_toplevel(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        self.sync_window(toplevel);
    }

    fn toplevel_closed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        let Some(id) = self.ids.remove(toplevel) else {
            return;
        };
        self.windows.closed(id);
        self.thumbnails.forget(id);
        self.forget_overlap(toplevel);
        if let Some(view) = self.switcher.take() {
            self.switcher = view.without(id);
        }
        self.windows_changed();
    }
}

impl WorkspaceHandler for Berth {
    fn workspace_state(&mut self) -> &mut WorkspaceState {
        &mut self.workspaces
    }

    fn done(&mut self) {
        for toplevel in self.ids.keys().cloned().collect::<Vec<_>>() {
            self.sync_window(&toplevel);
        }
    }
}

impl ToplevelManagerHandler for Berth {
    fn toplevel_manager_state(&mut self) -> &mut ToplevelManagerState {
        self.manager
            .as_mut()
            .expect("capabilities only arrive from a bound manager")
    }

    fn capabilities(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: Vec<WEnum<zcosmic_toplevel_manager_v1::ZcosmicToplelevelManagementCapabilitiesV1>>,
    ) {
    }
}

impl CompositorHandler for Berth {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &WlSurface,
        factor: i32,
    ) {
        self.rescale(surface, factor.max(1) as u32 * canvas::WHOLE, false);
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &WlSurface, _: u32) {
        self.dock_frame(surface);
        self.menu_frame(surface);
        if let Some(view) = &mut self.switcher {
            view.frame(surface);
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for Berth {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if self.switcher.as_ref().is_some_and(|view| view.owns(layer)) {
            self.switcher = None;
        } else {
            log::warn!("the compositor closed a dock");
            self.close_docks(|dock| dock.owns(layer));
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if self.switcher.as_ref().is_some_and(|view| view.owns(layer)) {
            self.switcher_configured(configure.new_size);
        } else {
            self.dock_configured(layer, configure.new_size);
        }
    }
}

impl SeatHandler for Berth {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seats.get_pointer(qh, &seat).ok();
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
        }
        self.seat.get_or_insert(seat);
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Pointer => self
                .pointer
                .take()
                .into_iter()
                .for_each(|pointer| pointer.release()),
            Capability::Keyboard => self
                .keyboard
                .take()
                .into_iter()
                .for_each(|keyboard| keyboard.release()),
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seat.take_if(|known| *known == seat);
    }
}

impl PointerHandler for Berth {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if let Some(at) = self.dock_at(&event.surface) {
                self.dock_pointer(at, event);
            } else if self.menu.as_ref().is_some_and(|menu| menu.is_on(&event.surface)) {
                self.menu_pointer(event);
            } else {
                self.switcher_pointer(event);
            }
        }
    }
}

impl KeyboardHandler for Berth {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        surface: &WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        if self.dock_at(surface).is_some() {
            self.dock_focused(surface);
        } else if !self.menu.as_ref().is_some_and(|menu| menu.is_on(surface)) {
            self.switcher_focused();
        }
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlKeyboard, _: &WlSurface, _: u32) {}

    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlKeyboard, _: u32, event: KeyEvent) {
        match self.menu {
            Some(_) => self.menu_key(event.keysym),
            None => self.switcher_key(event.keysym),
        }
    }

    fn repeat_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlKeyboard, _: u32, event: KeyEvent) {
        match self.menu {
            Some(_) => self.menu_key(event.keysym),
            None => self.switcher_key(event.keysym),
        }
    }

    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlKeyboard, _: u32, _: KeyEvent) {}

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
        self.dock_modifiers();
        self.switcher_modifiers();
    }
}

impl OutputHandler for Berth {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.open_dock(output);
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.output_changed(&output);
    }

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.close_docks(|dock| dock.is_for(&output));
    }
}

impl Dispatch<ZxdgDecorationManagerV1, GlobalData> for Berth {
    fn event(
        _: &mut Self,
        _: &ZxdgDecorationManagerV1,
        _: <ZxdgDecorationManagerV1 as wayland_client::Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl PopupHandler for Berth {
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup, _: PopupConfigure) {
        self.menu_configured(popup);
    }

    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        self.menu_dismissed(popup);
    }
}

impl ShmHandler for Berth {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Berth {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

impl Dispatch<WpFractionalScaleV1, WlSurface> for Berth {
    fn event(
        berth: &mut Self,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        surface: &WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            berth.rescale(surface, scale, true);
        }
    }
}

delegate_compositor!(Berth);
delegate_subcompositor!(Berth);
delegate_output!(Berth);
delegate_shm!(Berth);
delegate_seat!(Berth);
delegate_pointer!(Berth);
delegate_keyboard!(Berth);
delegate_layer!(Berth);
delegate_dispatch!(Berth: [XdgWmBase: GlobalData] => XdgShell);
delegate_xdg_popup!(Berth);
delegate_registry!(Berth);
delegate_toplevel_info!(Berth);
delegate_toplevel_manager!(Berth);
delegate_workspace!(Berth);
delegate_noop!(Berth: WpViewporter);
delegate_noop!(Berth: WpViewport);
delegate_noop!(Berth: WpFractionalScaleManagerV1);
delegate_noop!(Berth: ignore ExtBackgroundEffectManagerV1);
delegate_noop!(Berth: ExtBackgroundEffectSurfaceV1);
delegate_noop!(Berth: ZcosmicOverlapNotifyV1);
delegate_noop!(Berth: ignore WlBuffer);
