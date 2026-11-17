# berth

A dock and an Alt+Tab window switcher for the COSMIC desktop. It is a normal
Wayland client: no compositor patch, no fork of COSMIC.

- **Dock**: one on every monitor, translucent, blurred by the compositor, icons
  swell under the pointer. It hides only when a window actually needs its space
  and returns when the pointer touches the bottom edge. What each click and
  scroll does is yours to decide, and a right-click opens a menu.
- **Switcher**: hold Alt and tap Tab for a row of live window thumbnails over a
  dimmed, blurred desktop. Release Alt to switch.

It works with no configuration: the dock starts with the apps pinned to the
COSMIC dock and the icon theme chosen in COSMIC Settings.

## Install

```sh
cargo install --path .
```

Start it by hand with `berth`, or at login as a user service:

```sh
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/berth.service <<'UNIT'
[Unit]
Description=berth dock and window switcher
PartOf=graphical-session.target
After=graphical-session.target

[Service]
ExecStart=%h/.cargo/bin/berth
Restart=on-failure
KillMode=process

[Install]
WantedBy=graphical-session.target
UNIT
systemctl --user enable --now berth.service
```

`Restart=on-failure` brings berth back if it crashes or the compositor restarts.
`KillMode=process` keeps the apps you started from the dock running through
such a restart. Plugging and unplugging monitors needs no restart: each monitor
gets a dock when it appears and loses it when it goes.

### Replace COSMIC's own dock and switcher

Turn the COSMIC dock off in Settings → Desktop → Dock.

Alt+Tab stays COSMIC's shortcut; only the command behind it changes. COSMIC reads
that command from a `system_actions` file, and a user copy replaces the system
one as a whole, so copy it and change two lines:

```sh
dir=~/.config/cosmic/com.system76.CosmicSettings.Shortcuts/v1
mkdir -p "$dir"
sed -e "s|cosmic-launcher alt-tab|$(command -v berth) switch next|" \
    -e "s|cosmic-launcher shift-alt-tab|$(command -v berth) switch previous|" \
    /usr/share/cosmic/com.system76.CosmicSettings.Shortcuts/v1/system_actions \
    > "$dir/system_actions"
```

Delete that file to get COSMIC's switcher back.

## Configure

`~/.config/berth/config.toml`. Every key is optional; these are the defaults.

```toml
[dock]
edge = "bottom"           # "bottom", "top", "left" or "right"
icon-size = 48            # 16 to 256
magnification = 1.6       # 1.0 turns the swell off, up to 3.0
opacity = 0.55            # 0.0 clear to 1.0 solid
blur = true
autohide = "intelligent"  # "intelligent", "always" or "never"
windows = "all"           # "this-monitor": each dock shows only its monitor's windows
# pinned = ["firefox", "org.gnome.Nautilus"]   # see "Pinned apps" below

# [dock.monitor.DP-3]     # pins for one monitor only, by its output name
# pinned = ["mpv", "steam"]

[dock.actions]
click = "toggle"
shift-click = "launch"
ctrl-click = "close"
middle-click = "launch"
right-click = "menu"
scroll-up = "cycle-back"
scroll-down = "cycle"

[animation]
enabled = true            # false: everything moves in a single step
speed = 1.0               # 0.25 (slow) to 4.0 (fast)
launch = "bounce"         # "bounce" or "none": the hop of a starting app's icon

[switcher]
opacity = 0.45            # how dark the desktop gets behind the row
blur = true
windows = "all"           # "this-workspace": leave out windows on other workspaces
```

| Action | On a running app | On an app with no windows |
| --- | --- | --- |
| `launch` | starts another instance | starts it |
| `focus` | raises its most recently used window | starts it |
| `toggle` | raises it, or minimizes it if it already has focus | starts it |
| `cycle`, `cycle-back` | steps through its windows | starts it |
| `minimize` | minimizes all its windows | nothing |
| `close` | closes its most recently used window | nothing |
| `quit` | closes all its windows | nothing |
| `menu` | opens the menu: its windows, New Window, Pin/Unpin, Quit | opens the menu: Open, Unpin |
| `none` | nothing | nothing |

A misspelled key, an unknown action or a value out of range stops startup with a
message naming it.

`berth reload` reads the file again without a restart. If the new file has a
mistake, berth says so and keeps running with the old settings.

### Pinned apps

Without `pinned` in the file, the dock starts from the COSMIC dock's favorites
and "Pin to Dock" / "Unpin from Dock" in the menu change it; the result is kept
in `~/.local/state/berth/pinned`. With `pinned` in the file, the file decides and
the menu does not offer pinning.

A monitor with its own `[dock.monitor.NAME]` pins always follows the file, and
its menu does not offer pinning. `cosmic-randr list` shows the output names.

In the menu, the arrow keys move, Enter chooses, and Escape or a click anywhere
else closes it.

### Shift-click and Ctrl-click

COSMIC tells a program about modifier keys only while it has the keyboard, so
berth borrows the keyboard for the length of a left click and hands it straight
back. To avoid even that, give `shift-click` and `ctrl-click` the same action as
`click`; berth then never asks for the keyboard.

`berth windows` lists the windows berth tracks with the app id it matched them
to, most recently used first. Those ids are what `pinned` takes.

In the switcher, Tab and the arrow keys move, Shift+Tab moves back, Enter or
releasing the modifier switches, Escape or a click on the backdrop cancels, and
a click on a window switches to it.

## Develop

```sh
bin/verify
```

runs the formatter check, clippy with warnings as errors, the unit specs and the
end-to-end scenarios. The scenarios boot a private, headless desktop (sway
hosting a nested `cosmic-comp`) and drive it with a virtual pointer and
keyboard, so they need `sway`, `cosmic-comp`, `cosmic-randr`, `grim` and
`dbus-run-session` installed. They never touch the session you are sitting in.

## Not done

- A dock with so many apps that it does not fit even at the smallest icon size
  (16, without the swell) runs off the ends of the monitor.
- `windows = "this-monitor"` is covered by unit specs only: the nested COSMIC used
  for the end-to-end scenarios has one output, and sway, used for the
  multi-monitor scenarios, does not say which monitor a window is on.
