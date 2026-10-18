use std::os::fd::AsRawFd;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use ashpd::desktop::{
    screencast::{CursorMode, Screencast, SourceType},
    PersistMode,
};
use gst::prelude::*;
use gstreamer as gst;

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
    message.push_str("See the \"Installing dependencies\" section of the README.");
    bail!(message)
}

#[tokio::main]
async fn main() -> Result<()> {
    gst::init()?;
    check_elements()?;

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

    // 3. Build the GStreamer pipeline: PipeWire -> H.264 -> MKV
    //    x264enc defaults to a 2 Mbit/s constant bitrate, far too low for screen
    //    content: use constant-quality mode instead (pass=qual, quantizer = CRF,
    //    lower is sharper; 18 is visually near-lossless).
    let pipeline_desc = format!(
        "pipewiresrc fd={fd} path={node_id} do-timestamp=true keepalive-time=1000 resend-last=true \
         ! videoconvert ! queue \
         ! x264enc tune=zerolatency speed-preset=veryfast pass=qual quantizer=18 \
         ! h264parse ! matroskamux ! filesink location=capture.mkv",
        fd = fd.as_raw_fd(),
    );
    let pipeline = gst::parse::launch(&pipeline_desc)?
        .downcast::<gst::Pipeline>()
        .map_err(|_| anyhow::anyhow!("the root element must be a pipeline"))?;
    let bus = pipeline.bus().context("pipeline has no bus")?;

    pipeline.set_state(gst::State::Playing)?;
    println!("Recording to capture.mkv… Press Ctrl+C to stop.");

    // 4. Record until Ctrl+C, watching for errors
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    loop {
        tokio::select! {
            _ = &mut ctrl_c => break,
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

    drop(fd); // keep the PipeWire connection open until here
    println!("Done: capture.mkv");
    Ok(())
}
