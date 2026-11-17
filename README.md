# berth

A dock and an Alt+Tab window switcher for the COSMIC desktop. It is a normal
Wayland client: no compositor patch, no fork of COSMIC.

- **Dock**: translucent, blurred by the compositor, icons swell under the pointer.
  It hides only when a window actually needs its space and returns when the
  pointer touches the bottom edge. What each click and scroll does is yours to
  decide.
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

`Restart=on-failure` brings the dock back when the compositor closes it, which
it does when the monitor the dock is on goes away. `KillMode=process` keeps the
apps you started from the dock running through such a restart.

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
icon-size = 48            # 16 to 256
magnification = 1.6       # 1.0 turns the swell off, up to 3.0
opacity = 0.55            # 0.0 clear to 1.0 solid
blur = true
autohide = "intelligent"  # "intelligent", "always" or "never"
# pinned = ["firefox", "org.gnome.Nautilus"]   # default: the COSMIC dock's favorites

[dock.actions]
click = "toggle"
middle-click = "launch"
right-click = "minimize"
scroll-up = "cycle-back"
scroll-down = "cycle"

[switcher]
opacity = 0.45            # how dark the desktop gets behind the row
blur = true
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
| `none` | nothing | nothing |

A misspelled key, an unknown action or a value out of range stops startup with a
message naming it.

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

- One dock, on the output the compositor picks. No dock per monitor.
- No right-click menu, and no pinning from the dock: edit `pinned`.
- No Shift-click or Ctrl-click gestures. COSMIC tells a client about modifier
  keys only while it has keyboard focus, and a dock that takes focus steals it
  from your windows.
- The dock sits at the bottom edge only.
- No launch animation while an app starts.
- The switcher lists every window, not only those on the current workspace.
- Configuration is read at startup; restart berth after changing it.
