# Working on berth

## Definition of done

`bin/verify` green, run after the last edit, with its real output reported.
Never skip, delete or weaken a scenario to get there.

## Working agreements

- Test first. Name every test as a sentence about what the user sees, never
  after the function it calls. A bug fix starts with a failing scenario.
- Every user-visible behavior gets an end-to-end scenario in `tests/` against
  the real compositor, in the same change. Unit specs do not replace it.
- Cover failure paths: bad configuration, missing compositor features, windows
  that vanish mid-gesture.
- Add the smallest honest mechanism. No speculative flags. A feature that
  cannot work on COSMIC is removed, not shipped half-working.
- Reach for a maintained crate before writing a parser, a protocol binding or
  an event loop.
- Code explains itself in the domain's words (dock, pin, gesture, reveal,
  switcher, stage, cover). No comments. The specs are the documentation.
- Newest toolchain and dependencies, nightly included. Fix breakage, do not pin
  back.
- Say plainly what is not done or not verified. `README.md` keeps that list.

## Layout

- `src/dock`, `src/switcher`, `src/windows.rs`, `src/config.rs`: pure rules and
  painting, no Wayland. Fast specs live beside the code.
- `src/shell`: the Wayland client. Thin: it feeds events to the pure modules and
  presents what they paint.
- `tests/support`: boots the private desktop and injects input.
- `examples/window.rs`: the coloured test window the scenarios open.

## Things that bite

- A nested `cosmic-comp` imports its `WAYLAND_DISPLAY` into the systemd user
  manager and the D-Bus activation environment. Run it only the way
  `tests/support` does: private `XDG_RUNTIME_DIR`, `dbus-run-session`, and the
  no-op `systemctl` shim first in `PATH`. Anything less breaks app launching in
  the real session.
- Never store a value whose destructor sends a Wayland request (a `RawPool`, a
  `SlotPool`, a `Region`) in a proxy's user data. `wayland-backend` drops user
  data while holding its protocol lock when the compositor retires the object,
  and the request then deadlocks the whole client.
- COSMIC silently ignores `notify_on_overlap` for a layer surface that has not
  been mapped yet. Subscribe after the first buffer is committed.
- Keyboard modifiers reach a client only while it has keyboard focus. The dock
  asks for exclusive keyboard focus on a left press and gives it up at the
  click; it acts on the windows as they were at the press, because focus moves
  in between.
- A nested COSMIC has one output. Scenarios about several monitors run berth
  straight on headless sway (`Desktop::on_sway_with_monitors`), which has no
  COSMIC protocols: no window states, no activation.
- The menu is an xdg popup that grabs with the serial of the button press that
  opened it, although it opens on the release. COSMIC accepts that, gives the
  popup the keyboard and dismisses it on a click elsewhere.
- A hovered icon shows its name where a menu would be. Scenarios that look for
  the menu sample only its left half and park the pointer on an icon to its
  right.
- The dock is laid out lying along the bottom edge; `Edge::place` turns that into
  the other three edges. Do region and hit-test arithmetic in the lying layout
  and place the result.
- The event loop takes SIGTERM and SIGINT through a signalfd, which blocks them
  for the process. A child inherits that mask, so `apps::launch` clears it
  before exec. Any new way of starting a program must do the same.
- Window pictures for the switcher are taken when a window loses focus, not when
  the switcher opens, so opening it costs no capture for windows already seen.
- `overflow-checks` in the release profile made every painter 1.5 to 1.8 times
  slower in one measurement taken on a loaded machine; it is left off.
- `cargo test --test NAME` does not rebuild examples; the harness builds the
  test window itself for that reason.
- `/tmp` is mounted `noexec` on the machine this was written on. Anything the
  scenarios execute lives under `target/tmp`.
