mod icon;
mod tray;

use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use ashpd::desktop::{
    global_shortcuts::{GlobalShortcuts, NewShortcut},
    screencast::{CursorMode, Screencast, SourceType},
    PersistMode,
};
use futures_util::StreamExt;
use gst::prelude::*;
use gstreamer as gst;
use ksni::TrayMethods;
use tokio::sync::mpsc::{self, UnboundedSender};

use tray::{Command, KaptTray};

/// Application ID registered with the portals (needed by KDE to bind global shortcuts).
const APP_ID: &str = "io.github.olivierchoquet.Kapt";

/// Global shortcut that toggles pause/resume.
const SHORTCUT_PAUSE: &str = "toggle-pause";

/// Global shortcut that stops the recording (same as Ctrl+C).
const SHORTCUT_STOP: &str = "stop";

/// GStreamer elements used by the pipeline, with the plugin set that provides them.
/// Core elements (`queue`, `filesink`) are always available and are not listed.
const REQUIRED_ELEMENTS: &[(&str, &str)] = &[
    (
        "pipewiresrc",
        "the PipeWire plugin (gstreamer1.0-pipewire / gst-plugin-pipewire)",
    ),
    ("videoconvert", "gst-plugins-base"),
    ("x264enc", "gst-plugins-ugly"),
    ("h264parse", "gst-plugins-bad"),
    ("matroskamux", "gst-plugins-good"),
];

/// Checks that every pipeline element is available before opening the portal
/// dialog, and reports the missing plugins.
fn check_elements() -> Result<()> {
    let missing: Vec<_> = REQUIRED_ELEMENTS
        .iter()
        .filter(|(element, _)| gst::ElementFactory::find(element).is_none())
        .collect();
    if missing.is_empty() {
        return Ok(());
    }

    let mut message = String::from("missing GStreamer elements:\n");
    for (element, plugin) in missing {
        message.push_str(&format!("  - {element} (provided by {plugin})\n"));
    }
    message.push_str("See the \"Installation\" section of the README.");
    bail!(message)
}

/// Pause bookkeeping shared between the main task and the pad probe.
///
/// While paused, frames are dropped. On resume, the pause duration is added to
/// `offset`, which is subtracted from every following timestamp: the recording
/// continues seamlessly instead of showing a frozen frame for the whole pause.
#[derive(Default)]
struct PauseState {
    paused: bool,
    /// Running time at which the current pause started.
    paused_at: gst::ClockTime,
    /// Running time of the last resume: older frames still in flight are dropped.
    resumed_at: gst::ClockTime,
    /// Total duration of all pauses so far.
    offset: gst::ClockTime,
}

/// Installs a probe on the source pad that drops frames while paused and
/// shifts the timestamps of the following ones.
fn install_pause_probe(pipeline: &gst::Pipeline, state: Arc<Mutex<PauseState>>) -> Result<()> {
    let pad = pipeline
        .by_name("src")
        .and_then(|src| src.static_pad("src"))
        .context("cannot find the source pad of pipewiresrc")?;

    pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        let Ok(state) = state.lock() else {
            return gst::PadProbeReturn::Ok;
        };
        let Some(buffer) = info.buffer_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        let Some(pts) = buffer.pts() else {
            return gst::PadProbeReturn::Ok;
        };

        if state.paused || pts < state.resumed_at {
            return gst::PadProbeReturn::Drop;
        }
        if !state.offset.is_zero() {
            let buffer = buffer.make_mut();
            buffer.set_pts(pts.saturating_sub(state.offset));
            if let Some(dts) = buffer.dts() {
                buffer.set_dts(dts.saturating_sub(state.offset));
            }
        }
        gst::PadProbeReturn::Ok
    });
    Ok(())
}

/// Switches between recording and paused, using the pipeline running time.
/// Returns `true` if the recording is now paused.
fn toggle_pause(pipeline: &gst::Pipeline, state: &Mutex<PauseState>) -> Result<bool> {
    let now = pipeline
        .current_running_time()
        .context("the pipeline has no running time")?;
    let mut state = state
        .lock()
        .map_err(|_| anyhow!("pause state lock poisoned"))?;

    if state.paused {
        let pause = now.saturating_sub(state.paused_at);
        state.offset += pause;
        state.resumed_at = now;
        state.paused = false;
        println!("⏺ Recording (paused for {:.1} s)", pause.seconds_f64());
    } else {
        state.paused_at = now;
        state.paused = true;
        println!("⏸ Paused");
    }
    Ok(state.paused)
}

/// Binds the pause/resume and stop global shortcuts through the GlobalShortcuts portal,
/// then forwards their activations as commands to the main loop.
/// The first time, KDE asks the user to confirm (or change) the key combinations.
async fn bind_shortcuts(commands: UnboundedSender<Command>) -> Result<()> {
    let proxy = GlobalShortcuts::new().await?;
    let session = proxy.create_session().await?;
    let shortcuts = [
        NewShortcut::new(SHORTCUT_PAUSE, "Pause/resume recording")
            .preferred_trigger("LOGO+SHIFT+p"),
        NewShortcut::new(SHORTCUT_STOP, "Stop recording").preferred_trigger("LOGO+SHIFT+x"),
    ];
    let bound = proxy
        .bind_shortcuts(&session, &shortcuts, None)
        .await?
        .response()?;

    for shortcut in bound.shortcuts() {
        println!(
            "Shortcut: {} → {}",
            shortcut.trigger_description(),
            shortcut.description()
        );
    }

    let mut events = proxy.receive_activated().await?;
    // The task owns the proxy and session, which must stay alive as long as
    // the shortcuts are used
    tokio::spawn(async move {
        let _keep_alive = (&proxy, &session);
        while let Some(event) = events.next().await {
            let command = match event.shortcut_id() {
                SHORTCUT_PAUSE => Command::TogglePause,
                SHORTCUT_STOP => Command::Stop,
                _ => continue,
            };
            if commands.send(command).is_err() {
                break; // main loop finished
            }
        }
    });
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    gst::init()?;
    check_elements()?;

    // 0. Register kapt with the portals (must come before any other portal call)
    let app_id = APP_ID.parse().context("invalid application ID")?;
    if let Err(err) = ashpd::register_host_app(app_id).await {
        eprintln!("warning: cannot register with the portals: {err}");
    }

    // Commands from the global shortcuts and the tray icon
    let (commands, mut command_rx) = mpsc::unbounded_channel();

    // Global shortcuts are optional: without them, recording still works
    if let Err(err) = bind_shortcuts(commands.clone()).await {
        eprintln!(
            "warning: global shortcuts unavailable, pause and stop shortcuts disabled: {err:#}"
        );
    }

    // 1. Ask the portal for permission to capture the screen
    let proxy = Screencast::new().await?;
    let session = proxy.create_session().await?;
    proxy
        .select_sources(
            &session,
            CursorMode::Embedded, // cursor embedded in the video
            SourceType::Monitor | SourceType::Window, // whole screen or a window
            false,                // a single source
            None,                 // no restore token
            PersistMode::DoNot,
        )
        .await?;

    // Opens the KDE source selection dialog
    let response = proxy.start(&session, None).await?.response()?;
    let stream = response
        .streams()
        .first()
        .context("the portal returned no stream")?;
    let node_id = stream.pipe_wire_node_id();

    // 2. Get the PipeWire connection provided by the portal
    let fd = proxy.open_pipe_wire_remote(&session).await?;

    // Output file named after the local start time, e.g. 2026-10-18_14-32-05_kapt.mkv
    // (no ':' so the name stays valid on every filesystem)
    let output = gst::glib::DateTime::now_local()
        .and_then(|now| now.format("%Y-%m-%d_%H-%M-%S_kapt.mkv"))
        .context("cannot build the output file name from the current time")?;

    // 3. Build the GStreamer pipeline: PipeWire -> H.264 -> MKV
    //    pipewiresrc is named "src" so the pause probe can find it.
    //    x264enc defaults to a 2 Mbit/s constant bitrate, far too low for screen
    //    content: use constant-quality mode instead (pass=qual, quantizer = CRF,
    //    lower is sharper; 18 is visually near-lossless).
    let pipeline_desc = format!(
        "pipewiresrc name=src fd={fd} path={node_id} do-timestamp=true keepalive-time=1000 resend-last=true \
         ! videoconvert ! queue \
         ! x264enc tune=zerolatency speed-preset=veryfast pass=qual quantizer=18 \
         ! h264parse ! matroskamux ! filesink location=\"{output}\"",
        fd = fd.as_raw_fd(),
    );
    let pipeline = gst::parse::launch(&pipeline_desc)?
        .downcast::<gst::Pipeline>()
        .map_err(|_| anyhow!("the root element must be a pipeline"))?;
    let bus = pipeline.bus().context("pipeline has no bus")?;

    let pause_state = Arc::new(Mutex::new(PauseState::default()));
    install_pause_probe(&pipeline, Arc::clone(&pause_state))?;

    pipeline.set_state(gst::State::Playing)?;
    println!("Recording to {output}… Press Ctrl+C to stop.");

    // Tray icon showing the recording state; optional as well
    let tray = match KaptTray::new(output.to_string(), commands).spawn().await {
        Ok(handle) => Some(handle),
        Err(err) => {
            eprintln!("warning: cannot show the tray icon: {err}");
            None
        }
    };

    // 4. Record until Ctrl+C or a stop command, watching for errors and pause/resume
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    loop {
        tokio::select! {
            _ = &mut ctrl_c => break,
            Some(command) = command_rx.recv() => match command {
                Command::TogglePause => {
                    let paused = toggle_pause(&pipeline, &pause_state)?;
                    if let Some(tray) = &tray {
                        tray.update(|tray| tray.paused = paused).await;
                    }
                }
                Command::Stop => break,
            },
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                if let Some(msg) = bus.pop_filtered(&[gst::MessageType::Error]) {
                    if let gst::MessageView::Error(err) = msg.view() {
                        pipeline.set_state(gst::State::Null)?;
                        bail!("GStreamer error: {} ({:?})", err.error(), err.debug());
                    }
                }
            }
        }
    }

    // 5. Send EOS so the file is finalized properly
    println!("\nStopping, finalizing the file…");
    pipeline.send_event(gst::event::Eos::new());
    bus.timed_pop_filtered(
        gst::ClockTime::from_seconds(5),
        &[gst::MessageType::Eos, gst::MessageType::Error],
    );
    pipeline.set_state(gst::State::Null)?;

    if let Some(tray) = tray {
        tray.shutdown().await;
    }
    drop(fd); // keep the PipeWire connection open until here
    println!("Done: {output}");
    Ok(())
}
