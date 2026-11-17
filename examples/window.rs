use std::env;

use cosmic_client_toolkit::sctk::compositor::{CompositorHandler, CompositorState};
use cosmic_client_toolkit::sctk::output::{OutputHandler, OutputState};
use cosmic_client_toolkit::sctk::registry::{ProvidesRegistryState, RegistryState};
use cosmic_client_toolkit::sctk::shell::WaylandSurface;
use cosmic_client_toolkit::sctk::shell::xdg::XdgShell;
use cosmic_client_toolkit::sctk::shell::xdg::window::{
    Window, WindowConfigure, WindowDecorations, WindowHandler,
};
use cosmic_client_toolkit::sctk::shm::slot::SlotPool;
use cosmic_client_toolkit::sctk::shm::{Shm, ShmHandler};
use cosmic_client_toolkit::sctk::{
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_shm, wl_surface::WlSurface};
use wayland_client::{Connection, QueueHandle};

struct Swatch {
    registry: RegistryState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    colour: [u8; 3],
    size: (u32, u32),
    closed: bool,
}

fn main() {
    let arguments: Vec<String> = env::args().collect();
    let [_, app_id, title, colour, placement] = arguments.as_slice() else {
        eprintln!("usage: window APP_ID TITLE RRGGBB WIDTHxHEIGHT|maximized");
        std::process::exit(2);
    };
    let colour = u32::from_str_radix(colour, 16)
        .expect("a colour like ff8800")
        .to_be_bytes();
    let size = (placement != "maximized").then(|| {
        let (wide, tall) = placement.split_once('x').expect("a size like 640x480");
        (wide.parse().expect("a width"), tall.parse().expect("a height"))
    });

    let connection = Connection::connect_to_env().expect("a Wayland compositor");
    let (globals, mut queue) = registry_queue_init(&connection).expect("the compositor's globals");
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let window = XdgShell::bind(&globals, &qh).expect("xdg-shell").create_window(
        compositor.create_surface(&qh),
        WindowDecorations::RequestClient,
        &qh,
    );
    window.set_app_id(app_id.as_str());
    window.set_title(title.as_str());
    if size.is_none() {
        window.set_maximized();
    }
    window.commit();

    let mut swatch = Swatch {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        pool: SlotPool::new(1 << 16, &shm).expect("shared memory"),
        shm,
        window,
        colour: [colour[1], colour[2], colour[3]],
        size: size.unwrap_or((640, 480)),
        closed: false,
    };
    while !swatch.closed {
        queue
            .blocking_dispatch(&mut swatch)
            .expect("the compositor connection");
    }
}

impl WindowHandler for Swatch {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.closed = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        configure: WindowConfigure,
        _: u32,
    ) {
        if let (Some(wide), Some(tall)) = configure.new_size {
            self.size = (wide.get(), tall.get());
        }
        let (wide, tall) = (self.size.0 as i32, self.size.1 as i32);
        let (buffer, canvas) = self
            .pool
            .create_buffer(wide, tall, wide * 4, wl_shm::Format::Xrgb8888)
            .expect("a buffer for the window");
        let [red, green, blue] = self.colour;
        canvas
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|pixel| pixel.copy_from_slice(&[blue, green, red, 255]));
        let surface = self.window.wl_surface();
        buffer.attach_to(surface).expect("a fresh buffer");
        surface.damage_buffer(0, 0, wide, tall);
        surface.commit();
    }
}

impl CompositorHandler for Swatch {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: i32) {}
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: u32) {}
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

impl OutputHandler for Swatch {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for Swatch {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Swatch {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}

delegate_compositor!(Swatch);
delegate_output!(Swatch);
delegate_shm!(Swatch);
delegate_xdg_shell!(Swatch);
delegate_xdg_window!(Swatch);
delegate_registry!(Swatch);
