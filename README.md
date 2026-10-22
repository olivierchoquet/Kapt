# kapt

Command-line screen recorder for Linux, designed primarily for **KDE Plasma** on **Wayland**.

> ⚠️ This project is a prototype.

## How it works

1. kapt asks the **ScreenCast portal** (`xdg-desktop-portal`) for permission; KDE shows a dialog where you pick a screen or window.
2. **KWin** publishes the selected screen as a **PipeWire** stream; the portal hands kapt a connection (`fd`) and the stream id (`node_id`). kapt never reads the screen directly.
3. A **GStreamer** pipeline turns that stream into an H.264 MKV file:

```
pipewiresrc → videocrop → videoscale → videoconvert → queue → x264enc → h264parse → matroskamux → filesink
```

| Element        | Role                                                         | Ubuntu package            |
|----------------|--------------------------------------------------------------|---------------------------|
| `pipewiresrc`  | connects to the PipeWire stream and receives raw frames      | `gstreamer1.0-pipewire`   |
| `videocrop`    | trims an odd last column/row: 4:2:0 needs an even frame size | `gstreamer1.0-plugins-good` |
| `videoscale`   | keeps the video at the size of its first frame: if a captured window is resized, fits the new frames in, with black borders | `gstreamer1.0-plugins-base` |
| `videoconvert` | converts the pixel format (RGB from the screen → YUV 4:2:0, readable by every player) | `gstreamer1.0-plugins-base` |
| `queue`        | buffer, so capture never waits for the encoder               | core                      |
| `x264enc`      | compresses frames to H.264 (constant quality, CRF 18)        | `gstreamer1.0-plugins-ugly` |
| `h264parse`    | formats the H.264 stream for the container                   | `gstreamer1.0-plugins-bad`  |
| `matroskamux`  | wraps the video in an MKV file, with timestamps              | `gstreamer1.0-plugins-good` |
| `filesink`     | writes the result to disk                                    | core                      |

kapt checks that these elements are available at startup and lists any missing ones.

### Stopping cleanly: EOS

An **EOS** (*end of stream*) is a special GStreamer event that travels through the pipeline like a frame, after the last one, and tells each element "nothing more is coming". Each element finishes its work before passing it on: the encoder outputs the frames it still holds, then `matroskamux` writes the file **index** (the table that lets players show the duration and seek). Without it, the MKV is truncated: no duration, no seeking, sometimes missing the last seconds.

When you stop the recording (Ctrl+C, shortcut or tray menu), kapt therefore pushes an EOS right after `pipewiresrc` and waits (up to 5 s) for it to reach `filesink`, before shutting the pipeline down. If the capture is stopped from outside (KDE's "stop sharing" indicator, captured window closed) or a GStreamer error occurs, kapt only logs the error: the file may be truncated.

## Installation

Requirements: a **Wayland** session (KDE Plasma 6 recommended) with **PipeWire** running, and **Rust** 1.80 or newer (via [rustup](https://rustup.rs)).

Example for Ubuntu / Kubuntu / KDE neon:

```bash
sudo apt install build-essential pkg-config libglib2.0-dev libgstreamer1.0-dev \
    gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly \
    gstreamer1.0-pipewire xdg-desktop-portal xdg-desktop-portal-kde
```

On other distributions, install the equivalent packages. On Fedora and openSUSE, `x264enc` comes from the **RPM Fusion** and **Packman** repositories respectively.

Then build:

```bash
cargo build --release
```

## Usage

```bash
./target/release/kapt
```

1. KDE shows a dialog: pick a screen or a window.
2. Recording starts into a file named after the start time (e.g. `2026-10-18_14-32-05_kapt.mkv`), in your videos folder (`~/Videos`, or its localized name such as `~/Vidéos`). A tray icon shows the state: a red square with a white "K" while recording, an orange square with pause bars while paused. Click it to pause/resume, or use its menu to stop.
3. Press **Meta+Shift+P** (global shortcut, works from any window) to pause or resume: paused parts are cut from the video.
4. Press **Meta+Shift+X** (global shortcut) or **Ctrl+C** in the terminal to stop.

The first time, KDE asks you to confirm the shortcuts; you can change it later in *System Settings → Keyboard → Shortcuts*.

### Options

| Option | Default | Effect |
|--------|---------|--------|
| `-o, --output <PATH>` | videos folder | Output MKV file, or directory in which the file is named after the start time. An existing file is never overwritten. |
| `-q, --quality <0-51>` | `18` | Constant quality (x264 CRF): lower is sharper but bigger; 18 is visually near-lossless, 23 is x264's default. |
| `-p, --preset <PRESET>` | `veryfast` | x264 speed preset, from `ultrafast` to `veryslow`: faster presets use less CPU but make bigger files. |
| `-r, --region <WxH+X+Y>` | whole source | Record only this part of the selected screen or window (see below). |
| `-P, --profile <NAME>` | | Use the options saved in this profile; options given on the command line win. |
| `--save-profile <NAME>` | | Save the options given (and those of `--profile`) as this profile, then record. |
| `--list-profiles` | | List the saved profiles and exit. |

```bash
kapt -o ~/demo.mkv -q 23 -p fast
```

### Recording part of a window: `--region`

`--region WIDTHxHEIGHT+X+Y` keeps only a rectangle of the screen or window picked in the KDE dialog, in pixels from its **top-left corner**:

```bash
kapt --region 1280x720+100+200   # 1280×720 pixels, starting 100 px from the left and 200 px from the top
```

- Coordinates are relative to the image of the selected source, as a recording without `--region` shows it: for a window, its own top-left corner, not the screen's.
- Width and height are rounded down to even numbers (4:2:0 encoding needs it).
- If the region goes beyond the source (e.g. the window was made smaller), kapt records the visible part and prints a warning; if it lies entirely outside, kapt records the whole source.

Tip: to measure a region, take a screenshot of the window with Spectacle and read the coordinates of the area in an image editor (GIMP shows the pointer position at the bottom left).

### Profiles

A profile saves options you use often, region included, under a name:

```bash
kapt --region 1280x720+100+200 -q 23 -p fast --save-profile demo   # save (and record)
kapt -P demo                                                         # reuse
kapt -P demo -q 18                                                   # reuse, with another quality
kapt --list-profiles
```

`--save-profile` records right after saving, so you can check the region straight away; if it is not right, run it again with other values: the profile is replaced. Only the options you give are saved, so a profile benefits from future changes of the defaults.

Profiles are stored in `~/.config/kapt/profiles.toml`, which you can also edit by hand (`~/` is allowed in `output`):

```toml
[profiles.demo]
region = "1280x720+100+200"
quality = 23
preset = "fast"
output = "~/Videos/demos"
```

Saving a profile with `--save-profile` rewrites the file: comments added by hand are lost.

If no dialog shows up, check that the portal is running:
`systemctl --user status xdg-desktop-portal xdg-desktop-portal-kde`.

## Roadmap

Done:

- [x] Screen or window capture through the ScreenCast portal and PipeWire
- [x] Unit tests for the pause timestamp logic, CI on GitHub Actions (format, clippy, tests)
- [x] Constant video size: a captured window resized while recording is scaled to fit, with black borders
- [x] H.264 encoding in constant-quality mode (CRF 18), 4:2:0 for player compatibility (odd window sizes trimmed by one pixel), MKV output finalized properly
- [x] Startup check of the required GStreamer elements, with the missing plugins listed
- [x] Output file named after the start time (`2026-10-18_14-32-05_kapt.mkv`), in the XDG videos folder by default
- [x] Command-line options: `--output`, `--quality` (x264 CRF), `--preset` (x264 speed preset)
- [x] Capture region inside the selected window or screen (`--region`), and named profiles to reuse options
- [x] Pause/resume and stop with global shortcuts (GlobalShortcuts portal), paused parts cut from the video
- [x] Tray icon showing the state (recording / paused), with pause and stop actions

Planned:

- [ ] Split the code into a desktop-independent core (portal, PipeWire, GStreamer, pause logic) and optional front-ends, to offer a plain command-line kapt for non-KDE environments (GNOME, wlroots…) without the tray icon and with optional global shortcuts
- [ ] More command-line options: codec, bitrate, FPS
- [ ] Optional 4:4:4 chroma (`video/x-raw,format=Y444`) for sharper colored text, at the cost of player compatibility
- [ ] Adapt encoding settings to the selected source: read its resolution from the portal stream and pick quality/preset accordingly (e.g. a faster preset for 4K to avoid dropped frames)
- [ ] Audio capture (microphone and system sound)
- [ ] Hardware encoding via VA-API (`vah264enc`)
- [ ] Remember the selected source (`restore_token`)
- [ ] QML graphical interface via cxx-qt
- [ ] Finalize the file properly when the capture is stopped from outside (KDE's "stop sharing" indicator, source window closed) or when a GStreamer error occurs, instead of leaving a truncated MKV (currently only logged)
- [ ] Log file in `~/.local/state/kapt/kapt.log` (XDG state folder): timestamped messages, as in the terminal, plus the full GStreamer details on errors
- [ ] Show the elapsed recording time (tray tooltip, terminal)
- [ ] Background mode: kapt stays in the tray and a global shortcut starts a new recording
- [ ] Interactive region selection: show a frame of the selected source and let the user draw the rectangle (instead of typing `--region`)
- [ ] Remember the selected source in a profile (`restore_token`), so `kapt -P demo` skips the KDE dialog
- [ ] Multi-monitor capture
- [ ] Direct MP4 output: `mp4mux fragment-duration=1000 fragment-mode=first-moov-then-finalise` writes fragments while recording (readable if interrupted) and rewrites a standard MP4 on EOS; the EOS wait (currently 5 s) must be raised for long recordings
- [ ] Other output formats: WebM/AV1, GIF for short clips
- [ ] Desktop file, AppStream metadata and application icon, so KDE shows "kapt" by name in the shortcut and sharing dialogs
- [ ] Packaging: Flatpak, AUR, .deb
- [ ] Translations (French first)

## Development

```bash
cargo fmt                                  # format
cargo clippy --all-targets -- -D warnings  # lint, tests included
cargo test                                 # unit tests (no graphical session needed)
```

The CI (`.github/workflows/ci.yml`) runs the same three checks on every push to `main` and every pull request.

## License

kapt is free software, licensed under the [GNU General Public License v3.0 or later](LICENSE) (GPL-3.0-or-later), like most KDE applications.
