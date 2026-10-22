//! Command-line options and output path resolution.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use gstreamer::glib;

/// Screen recorder for KDE Plasma on Wayland.
#[derive(Parser)]
#[command(version, about)]
pub struct Args {
    /// Output MKV file, or directory in which a file named after the start
    /// time is created [default: the XDG videos folder, e.g. ~/Videos]
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Constant quality (x264 CRF, 0-51): lower is sharper but bigger;
    /// 18 is visually near-lossless
    #[arg(short, long, default_value_t = 18, value_parser = clap::value_parser!(u8).range(0..=51))]
    pub quality: u8,

    /// x264 speed preset: faster presets use less CPU but make bigger files
    #[arg(short, long, value_enum, default_value_t = Preset::Veryfast)]
    pub preset: Preset,
}

/// x264 speed presets, from fastest to slowest.
#[derive(Clone, Copy, ValueEnum)]
pub enum Preset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    Medium,
    Slow,
    Slower,
    Veryslow,
}

impl Preset {
    /// Value of the `speed-preset` property of x264enc.
    pub fn as_str(self) -> &'static str {
        match self {
            Preset::Ultrafast => "ultrafast",
            Preset::Superfast => "superfast",
            Preset::Veryfast => "veryfast",
            Preset::Faster => "faster",
            Preset::Fast => "fast",
            Preset::Medium => "medium",
            Preset::Slow => "slow",
            Preset::Slower => "slower",
            Preset::Veryslow => "veryslow",
        }
    }
}

/// Where the recording is written.
pub enum Output {
    /// A file given by the user.
    File(PathBuf),
    /// A directory: the file is named after the start time.
    Dir(PathBuf),
}

impl Output {
    /// Resolves `--output` before the portal dialog opens, so a wrong path
    /// fails right away instead of after the user picked a source.
    pub fn resolve(arg: Option<PathBuf>) -> Result<Self> {
        let Some(path) = arg else {
            return Ok(Output::Dir(default_dir()?));
        };
        if path.is_dir() {
            return Ok(Output::Dir(path));
        }
        // Never overwrite an existing recording by mistake
        if path.exists() {
            bail!("{} already exists", path.display());
        }
        // `Path::parent` is empty for a bare file name: the current directory
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            if !parent.is_dir() {
                bail!("directory {} does not exist", parent.display());
            }
        }
        Ok(Output::File(path))
    }

    /// Path of the file to write, e.g. `~/Videos/2026-10-18_14-32-05_kapt.mkv`
    /// for a directory. Called when recording starts, so the name matches the
    /// start time rather than the launch time.
    pub fn file(self) -> Result<PathBuf> {
        match self {
            Output::File(path) => Ok(path),
            Output::Dir(dir) => {
                // No ':' in the name so it stays valid on every filesystem
                let name = glib::DateTime::now_local()
                    .and_then(|now| now.format("%Y-%m-%d_%H-%M-%S_kapt.mkv"))
                    .context("cannot build the output file name from the current time")?;
                Ok(dir.join(name.as_str()))
            }
        }
    }
}

/// The XDG videos folder (localized, e.g. ~/Vidéos in French), created if
/// missing; the current directory if the user has none configured.
fn default_dir() -> Result<PathBuf> {
    let Some(dir) = glib::user_special_dir(glib::UserDirectory::Videos) else {
        eprintln!("warning: no XDG videos folder configured, saving to the current directory");
        return Ok(PathBuf::from("."));
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    Ok(dir)
}
