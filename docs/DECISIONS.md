# Decision log

Problems solved and design decisions taken in kapt, newest first: what was
chosen, why, and what was ruled out. The git history says *what* changed;
this file says *why*.

Each entry: **Problem**, **Decision**, **Rejected** (alternatives considered),
**Consequences**, **Commit**. Unsolved problems go to [Open issues](#open-issues).

## 2026-10-22 — No countdown before recording

**Problem:** the roadmap planned an optional countdown before recording starts.

**Decision:** dropped for now: limited use. Shown on screen, it would end up
at the start of the video, like a notification; shown only in the terminal or
the tray, it adds little over starting kapt a few seconds early.

**Rejected:** a countdown in the terminal and the tray icon only.

**Consequences:** roadmap item removed; can come back if a need shows up.

**Commit:** see the commit adding this entry

## 2026-10-22 — No desktop notifications

**Problem:** the roadmap planned KDE notifications (recording started, paused,
finished) through the Notification portal.

**Decision:** dropped. A notification pops up on the screen being recorded,
so it would end up in the video. The tray icon (state) and the terminal
(messages, file name) already give the same information without getting in
the way.

**Rejected:** notifications only at the end, after the recording stopped:
little use, since the terminal already prints the file name.

**Consequences:** roadmap item removed.

**Commit:** see the commit adding this entry

## 2026-10-22 — Unit tests for the pause logic, CI

**Problem:** the pause timestamp logic, the most delicate code, was only
checked by hand, mixed with the pad probe and the pipeline.

**Decision:** move it to `src/pause.rs` as pure methods on `PauseState`
(`toggle(now)`, `shift(pts)`) that take and return timestamps; the probe and
`toggle_pause` only call them. Unit tests cover: no pause, frames dropped while
paused, shift after resume, frames in flight from before the resume, several
pauses, pause at start, instant pause, and increasing timestamps over a
simulated 10 s recording. CI on GitHub Actions (Ubuntu 24.04): `cargo fmt
--check`, `cargo clippy --all-targets -D warnings`, `cargo test`.

**Rejected:** testing with a real pipeline (`videotestsrc`) in CI: slower,
needs the GStreamer plugins, and the logic is what needed coverage.

**Consequences:** CI only installs `libgstreamer1.0-dev` (to compile and link),
not the plugins. Code that touches the pipeline or the portals is still tested
by hand.

**Commit:** see the commit adding this entry

## 2026-10-22 — Keep the video size fixed during a recording

**Problem:** resizing a captured window changes the frame size in the middle
of the file, which many players and editors handle badly.

**Decision:** the video keeps the size of its first frame (made even). Later
frames go through `videoscale add-borders=true`, which fits them into that size
while keeping their aspect ratio, with black borders. The size probe sets the
output caps (`width`, `height`, `pixel-aspect-ratio=1/1` so videoscale adds
borders instead of stretching) on the first caps event. While the size does
not change, videoscale does nothing, so the image stays sharp.

**Rejected:**
- Letting the size change in the file: the problem itself.
- Placing frames unscaled on a fixed canvas (cropped if bigger): a window
  that grows would be cut off.

**Consequences:** a window enlarged after the start is scaled down (slightly
blurrier text); one made smaller gets black borders.

**Commit:** see the commit adding this entry

## 2026-10-22 — Command-line options, recordings saved to the videos folder

**Problem:** the output path, quality and preset were hard-coded, and recordings
landed in whatever directory kapt was launched from.

**Decision:**
- `clap` (derive) parses `-o/--output`, `-q/--quality` (x264 CRF, 0-51,
  default 18) and `-p/--preset` (x264 speed preset, default `veryfast`).
- `--output` takes a file or a directory; it is checked before the portal
  dialog opens, so a wrong path fails right away.
- Without `--output`, the file goes to the XDG videos folder (created if
  missing), found with `glib::user_special_dir`: it follows the user's
  `user-dirs.dirs`, so it is localized (`~/Vidéos`). Fallback: the current
  directory, with a warning.
- An existing file is never overwritten: kapt refuses to start.
- The path is set on `filesink` as a property, not written into the pipeline
  description, so quotes or spaces in it cannot break the parsing.

**Rejected:**
- The `dirs` crate for the videos folder: glib (already pulled in by
  GStreamer) does the same.
- Hard-coding `~/Videos`: wrong on localized desktops.

**Consequences:** the file name is still taken when recording starts (after
the dialog), not at launch.

**Commit:** see the commit adding this entry

## 2026-10-22 — Encode in 4:2:0, trim odd frame sizes

**Problem:** recordings came out as H.264 *High 4:4:4 Predictive*: `videoconvert`
picks the format closest to the RGB screen, and many players (browsers, phones,
hardware decoders, some editors) cannot decode 4:4:4.

**Decision:** force 4:2:0 with `video/x-raw,format=I420` after `videoconvert`.
4:2:0 stores one chroma sample per 2×2 pixels, so x264 refuses odd frame sizes,
which windows can have: `videocrop` trims the odd last column and/or row. A
probe on the `pipewiresrc` pad reads each new frame size (caps event) and sets
the crop, so resizing the captured window mid-recording is handled.

**Rejected:**
- Keeping 4:4:4: sharper colored text, but poor player compatibility.
- `videoscale` to an even size: rescaling blurs text.

**Consequences:** odd-sized windows lose one pixel column/row. 4:4:4 remains a
planned option (see the roadmap). `videocrop` is checked at startup.

**Commit:** 828ce06

## 2026-10-22 — Handle only a user stop cleanly

**Problem:** when the capture is stopped from outside (KDE's "stop sharing"
indicator, captured window closed) or a GStreamer error occurs, the MKV is
left truncated.

**Decision:** only the stop requested by the user (Ctrl+C, shortcut, tray
menu) is finalized. Other cases log the error and say the file may be
truncated. These cases are rare; we will come back to them if they turn out
to matter.

**Rejected:** watching the portal session's `Closed` signal and finalizing
after a source error (EOS pushed after the dead source, ignoring the errors it
keeps posting). Implemented, then removed: too much complexity for rare cases,
and never tested against a real KDE session.

**Consequences:** the roadmap item "Finalize the file properly when the capture
is stopped from outside…" stays open.

**Commit:** e788f28

## 2026-10-22 — Push EOS from the pipewiresrc source pad

**Problem:** after Ctrl+C, the MKV had no duration and no index: the EOS sent to
the pipeline never reached `matroskamux` (timeout after 5 s). The old code
ignored the timeout, so the problem went unnoticed.

**Decision:** push EOS directly from the source pad of `pipewiresrc` instead of
`pipeline.send_event()`. The elements downstream get it, flush their buffers,
and `matroskamux` writes the index. Frames the source still pushes afterwards
are refused, which stops it.

**Rejected:** `pipeline.send_event(Eos)`, the usual way: `pipewiresrc` does
not forward it.

**Consequences:** checked on a real recording (duration readable with
`ffprobe`). A failed finalization now prints a warning.

**Commit:** e788f28

## Open issues

### `not-negotiated` error at startup (2026-10-22)

One recording failed right away with `Internal data stream error … reason
not-negotiated (-4)` from `pipewiresrc`, before any frame was written; the next
runs worked. An odd window size is **not** the cause (tested: x264 accepted
it, in 4:4:4). Cause unknown. If it happens again, run
`GST_DEBUG=3 cargo run 2> /tmp/kapt-debug.log` to see which formats were
offered and refused.
