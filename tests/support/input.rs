use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_pointer, wl_registry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};
use xkbcommon::xkb;

const SETTLE: Duration = Duration::from_millis(60);
const XKB_V1: u32 = 1;

#[derive(Clone, Copy)]
pub enum Button {
    Left = 0x110,
    Right = 0x111,
    Middle = 0x112,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Escape = 1,
    One = 2,
    Two = 3,
    Tab = 15,
    Enter = 28,
    Control = 29,
    Shift = 42,
    Alt = 56,
    Up = 103,
    Left = 105,
    Right = 106,
    Down = 108,
    Super = 125,
}

impl Key {
    fn modifier(self) -> u32 {
        match self {
            Key::Shift => 1,
            Key::Control => 4,
            Key::Alt => 8,
            Key::Super => 64,
            _ => 0,
        }
    }
}

struct Seated;

struct Devices {
    queue: EventQueue<Seated>,
    pointer: ZwlrVirtualPointerV1,
    keyboard: ZwpVirtualKeyboardV1,
    screen: (u32, u32),
    started: Instant,
    held: u32,
}

pub struct Host(Option<Devices>);

impl Host {
    pub fn unconnected() -> Self {
        Self(None)
    }

    pub fn connect(socket: &Path, screen: (u32, u32)) -> Self {
        let connection = Connection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<Seated>(&connection).unwrap();
        let qh = queue.handle();
        let seat: WlSeat = globals.bind(&qh, 1..=1, ()).unwrap();
        let pointers: ZwlrVirtualPointerManagerV1 = globals.bind(&qh, 1..=1, ()).unwrap();
        let keyboards: ZwpVirtualKeyboardManagerV1 = globals.bind(&qh, 1..=1, ()).unwrap();
        let pointer = pointers.create_virtual_pointer(Some(&seat), &qh, ());
        let keyboard = keyboards.create_virtual_keyboard(&seat, &qh, ());

        let keymap = xkb::Keymap::new_from_names(
            &xkb::Context::new(xkb::CONTEXT_NO_FLAGS),
            "",
            "",
            "us",
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .unwrap()
        .get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(keymap.as_bytes()).unwrap();
        file.write_all(&[0]).unwrap();
        keyboard.keymap(XKB_V1, file.as_fd(), keymap.len() as u32 + 1);
        queue.roundtrip(&mut Seated).unwrap();

        Self(Some(Devices {
            queue,
            pointer,
            keyboard,
            screen,
            started: Instant::now(),
            held: 0,
        }))
    }

    pub fn pointer(&mut self, x: f32, y: f32) {
        let devices = self.devices();
        let now = devices.now();
        devices
            .pointer
            .motion_absolute(now, x as u32, y as u32, devices.screen.0, devices.screen.1);
        devices.pointer.frame();
        devices.settle();
    }

    pub fn click(&mut self, button: Button) {
        let devices = self.devices();
        for state in [
            wl_pointer::ButtonState::Pressed,
            wl_pointer::ButtonState::Released,
        ] {
            devices.pointer.button(devices.now(), button as u32, state);
            devices.pointer.frame();
            devices.settle();
        }
    }

    pub fn scroll(&mut self, notches: i32) {
        let devices = self.devices();
        let now = devices.now();
        devices.pointer.axis_source(wl_pointer::AxisSource::Wheel);
        devices.pointer.axis_discrete(
            now,
            wl_pointer::Axis::VerticalScroll,
            15.0 * notches as f64,
            notches,
        );
        devices.pointer.frame();
        devices.settle();
    }

    pub fn letter(&mut self, letter: char) {
        const ROWS: [(&str, u32); 3] = [("qwertyuiop", 16), ("asdfghjkl", 30), ("zxcvbnm", 44)];
        let code = ROWS
            .iter()
            .find_map(|(row, first)| Some(first + row.find(letter)? as u32))
            .expect("a lowercase letter");
        let devices = self.devices();
        for down in [1, 0] {
            devices.keyboard.key(devices.now(), code, down);
            devices.settle();
        }
    }

    pub fn key(&mut self, key: Key, down: bool) {
        let devices = self.devices();
        devices.keyboard.key(devices.now(), key as u32, down as u32);
        if key.modifier() != 0 {
            devices.held = if down {
                devices.held | key.modifier()
            } else {
                devices.held & !key.modifier()
            };
            devices.keyboard.modifiers(devices.held, 0, 0, 0);
        }
        devices.settle();
    }

    fn devices(&mut self) -> &mut Devices {
        self.0.as_mut().expect("the test desktop is running")
    }
}

impl Devices {
    fn now(&self) -> u32 {
        self.started.elapsed().as_millis() as u32
    }

    fn settle(&mut self) {
        self.queue.roundtrip(&mut Seated).unwrap();
        sleep(SETTLE);
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Seated {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Seated: ignore WlSeat);
delegate_noop!(Seated: ZwlrVirtualPointerManagerV1);
delegate_noop!(Seated: ZwlrVirtualPointerV1);
delegate_noop!(Seated: ZwpVirtualKeyboardManagerV1);
delegate_noop!(Seated: ZwpVirtualKeyboardV1);
