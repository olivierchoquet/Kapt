# kapt

Command-line screen recorder for Linux, designed primarily for **KDE Plasma** on **Wayland**.

kapt uses the `xdg-desktop-portal` (ScreenCast API) to get permission to capture the screen, receives the video stream through **PipeWire**, then encodes it to H.264 in an MKV file with **GStreamer**.

```
ScreenCast portal → PipeWire → GStreamer (x264) → capture.mkv
```

> ⚠️ This project is a prototype.

## How it works

1. kapt asks the **ScreenCast portal** for permission; KDE shows a dialog where you pick a screen or window.
2. **KWin** publishes the selected screen as a **PipeWire** stream; the portal hands kapt a connection (`fd`) and the stream id (`node_id`). kapt never reads the screen directly.
3. A **GStreamer** pipeline turns that stream into a file:

```
pipewiresrc → videoconvert → queue → x264enc → h264parse → matroskamux → filesink
```

| Element        | Role |
|----------------|------|
| `pipewiresrc`  | connects to the PipeWire stream and receives raw frames |
| `videoconvert` | converts the pixel format (the screen gives RGB, the encoder wants YUV) |
| `queue`        | buffer, so capture never waits for the encoder |
| `x264enc`      | compresses frames to H.264 (constant quality, CRF 18) |
| `h264parse`    | formats the H.264 stream for the container |
| `matroskamux`  | wraps the video in an MKV file, with timestamps |
| `filesink`     | writes the result to disk |

On **Ctrl+C**, kapt sends an **EOS** (end of stream) through the pipeline: each element flushes its buffers and `matroskamux` writes the file index. Without it, the MKV would be corrupted or not seekable.

## Requirements

- A **Wayland** session (KDE Plasma 6 recommended; also works on X11 if the portal is available)
- **PipeWire** running
- **xdg-desktop-portal** and its KDE backend (`xdg-desktop-portal-kde`)
- **Rust** 1.80 or newer (via [rustup](https://rustup.rs))
- **pkg-config**, plus the **GLib** and **GStreamer** development headers
- The following GStreamer elements (kapt checks for them at startup):

| Element        | Provided by                                  |
|----------------|----------------------------------------------|
| `pipewiresrc`  | PipeWire plugin for GStreamer                |
| `videoconvert` | gst-plugins-base                             |
| `x264enc`      | gst-plugins-ugly                             |
| `h264parse`    | gst-plugins-bad                              |
| `matroskamux`  | gst-plugins-good                             |

## Installing dependencies

### Arch / Manjaro / EndeavourOS

```bash
sudo pacman -S --needed base-devel pkgconf \
    gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-plugin-pipewire \
    pipewire xdg-desktop-portal xdg-desktop-portal-kde
```

### Fedora

```bash
sudo dnf install gcc pkgconf-pkg-config glib2-devel gstreamer1-devel \
    gstreamer1-plugins-base gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-plugins-ugly \
    pipewire-gstreamer \
    xdg-desktop-portal xdg-desktop-portal-kde
```

> On Fedora, `x264enc` is provided by `gstreamer1-plugins-ugly` from the **RPM Fusion** repository.

### Debian / Ubuntu / Kubuntu / KDE neon

```bash
sudo apt install build-essential pkg-config libglib2.0-dev libgstreamer1.0-dev \
    gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly \
    gstreamer1.0-pipewire gstreamer1.0-tools xdg-desktop-portal xdg-desktop-portal-kde
```

### openSUSE

```bash
sudo zypper install gcc pkgconf glib2-devel gstreamer-devel \
    gstreamer-plugins-base gstreamer-plugins-good gstreamer-plugins-bad gstreamer-plugins-ugly \
    gstreamer-plugin-pipewire gstreamer-utils xdg-desktop-portal xdg-desktop-portal-kde
```

> On openSUSE, `x264enc` is provided by the **Packman** repository.

### Checking the installation

```bash
pkg-config --modversion gstreamer-1.0   # should print 1.2x
gst-inspect-1.0 pipewiresrc             # should print the element description
gst-inspect-1.0 x264enc
gst-inspect-1.0 h264parse
```

`gst-inspect-1.0` is provided by `gstreamer1.0-tools` (Debian/Ubuntu) or `gstreamer-utils` (openSUSE); it is included in the `gstreamer` package on Arch and `gstreamer1` on Fedora.

## Building

```bash
cargo build --release
```

The binary is then located at `target/release/kapt`.

## Usage

```bash
cargo run --release
# or
./target/release/kapt
```

1. KDE shows a dialog: pick a screen or a window.
2. Recording starts into `capture.mkv`, in the current directory.
3. Press **Ctrl+C** to stop: the file is finalized properly.

## Rust dependencies

| Crate       | Version | Purpose                                      |
|-------------|---------|----------------------------------------------|
| `ashpd`     | 0.11    | Talks to the ScreenCast portal (D-Bus)       |
| `gstreamer` | 0.23    | Capture and encoding pipeline                |
| `tokio`     | 1       | Async runtime, Ctrl+C handling               |
| `anyhow`    | 1       | Error handling                               |

Cargo reports newer versions of `ashpd` and `gstreamer`. Their APIs have changed: do not upgrade them without adapting the code.

## Troubleshooting

**`The pkg-config command could not be found`**
Install `pkg-config` (or `pkgconf`) along with the GLib and GStreamer development packages (see above).

**`missing GStreamer elements: x264enc`**
The "ugly" plugin set is not installed. As a workaround, replace the encoding part of the pipeline in `src/main.rs` with the line below, and update `REQUIRED_ELEMENTS` to match (`vp8enc`, `webmmux` instead of `x264enc`, `h264parse`, `matroskamux`):
```
! vp8enc deadline=1 ! webmmux ! filesink location=capture.webm
```

**`missing GStreamer elements: h264parse`**
The "bad" plugin set is not installed (`gst-plugins-bad`, `gstreamer1.0-plugins-bad`, `gstreamer1-plugins-bad-free`…).

**`missing GStreamer elements: pipewiresrc`**
Install the GStreamer PipeWire plugin (`gst-plugin-pipewire`, `gstreamer1.0-pipewire`, etc.).

**No dialog shows up**
Check that the portal is running: `systemctl --user status xdg-desktop-portal xdg-desktop-portal-kde`.

## Roadmap

- [ ] Command-line options (output file, codec, bitrate, FPS)
- [ ] Quality options: `--quality` (x264 CRF, currently fixed at 18) and `--preset` (x264 speed preset, currently `veryfast`)
- [ ] Optional 4:4:4 chroma (`video/x-raw,format=Y444`) for sharper colored text, at the cost of player compatibility
- [ ] Adapt encoding settings to the selected source: read its resolution from the portal stream and pick quality/preset accordingly (e.g. a faster preset for 4K to avoid dropped frames)
- [ ] Audio capture (microphone and system sound)
- [ ] Hardware encoding via VA-API (`vah264enc`)
- [ ] Remember the selected source (`restore_token`)
- [ ] QML graphical interface via cxx-qt

## License

kapt is free software, licensed under the [GNU General Public License v3.0 or later](LICENSE) (GPL-3.0-or-later), like most KDE applications.
