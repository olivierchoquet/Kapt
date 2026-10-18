//! System tray icon (StatusNotifierItem): shows whether kapt is recording or
//! paused, and offers pause/resume and stop actions.

use ksni::menu::StandardItem;
use ksni::{Category, Icon, MenuItem, Status, ToolTip, Tray};
use tokio::sync::mpsc::UnboundedSender;

use crate::icon;

/// Actions requested by the user, from the tray icon or a global shortcut.
#[derive(Debug, Clone, Copy)]
pub enum Command {
    TogglePause,
    Stop,
}

/// Tray icon state. Updated through `ksni::Handle::update`.
pub struct KaptTray {
    pub paused: bool,
    /// Output file name, shown in the tooltip.
    output: String,
    /// Channel to the main loop.
    commands: UnboundedSender<Command>,
    /// Icons drawn once at startup.
    recording_icon: Vec<Icon>,
    paused_icon: Vec<Icon>,
}

impl KaptTray {
    pub fn new(output: String, commands: UnboundedSender<Command>) -> Self {
        Self {
            paused: false,
            output,
            commands,
            recording_icon: icon::recording(),
            paused_icon: icon::paused(),
        }
    }

    fn current_icon(&self) -> Vec<Icon> {
        if self.paused {
            self.paused_icon.clone()
        } else {
            self.recording_icon.clone()
        }
    }

    fn send(&self, command: Command) {
        // The main loop may already be gone (recording finished): nothing to do
        let _ = self.commands.send(command);
    }

    fn state_label(&self) -> &'static str {
        if self.paused {
            "Paused"
        } else {
            "Recording"
        }
    }
}

impl Tray for KaptTray {
    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn category(&self) -> Category {
        Category::ApplicationStatus
    }

    fn status(&self) -> Status {
        // Active keeps the icon visible in the panel (not hidden in the overflow)
        Status::Active
    }

    fn title(&self) -> String {
        format!("kapt — {}", self.state_label())
    }

    // kapt's own icons, distinct from KDE's round "screen sharing" indicator
    fn icon_pixmap(&self) -> Vec<Icon> {
        self.current_icon()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: format!("kapt — {}", self.state_label()),
            description: self.output.clone(),
            icon_pixmap: self.current_icon(),
            ..Default::default()
        }
    }

    // Left click toggles pause/resume
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Command::TogglePause);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let (pause_label, pause_icon) = if self.paused {
            ("Resume", "media-record")
        } else {
            ("Pause", "media-playback-pause")
        };
        vec![
            StandardItem {
                label: pause_label.into(),
                icon_name: pause_icon.into(),
                activate: Box::new(|tray: &mut Self| tray.send(Command::TogglePause)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Stop recording".into(),
                icon_name: "media-playback-stop".into(),
                activate: Box::new(|tray: &mut Self| tray.send(Command::Stop)),
                ..Default::default()
            }
            .into(),
        ]
    }
}
