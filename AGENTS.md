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
- Keyboard modifiers reach a client only while it has keyboard focus.
- The event loop takes SIGTERM and SIGINT through a signalfd, which blocks them
  for the process. A child inherits that mask, so `apps::launch` clears it
  before exec. Any new way of starting a program must do the same.
- `cargo test --test NAME` does not rebuild examples; the harness builds the
  test window itself for that reason.
- `/tmp` is mounted `noexec` on the machine this was written on. Anything the
  scenarios execute lives under `target/tmp`.
