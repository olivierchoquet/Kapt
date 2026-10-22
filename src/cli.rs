//! Command-line options, combined with the selected profile, and output path
//! resolution.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use gstreamer::glib;
use serde::{Deserialize, Serialize};

use crate::profile::{expand_home, Profile, Profiles};
use crate::region::Region;

/// Constant quality used when neither the command line nor the profile sets one.
pub const DEFAULT_QUALITY: u8 = 18;

/// x264 speed preset used when neither the command line nor the profile sets one.
pub const DEFAULT_PRESET: Preset = Preset::Veryfast;

/// Screen recorder for KDE Plasma on Wayland.
// Options that a profile can hold have no clap default: an unset option lets
// the profile value through (see `Args::settings`).
#[derive(Parser)]
#[command(version, about)]
pub struct Args {
    /// Output MKV file, or directory in which a file named after the start
    /// time is created [default: the XDG videos folder, e.g. ~/Videos]
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Constant quality (x264 CRF, 0-51): lower is sharper but bigger;
    /// 18 is visually near-lossless [default: 18]
    #[arg(short, long, value_parser = clap::value_parser!(u8).range(0..=51))]
    pub quality: Option<u8>,

    /// x264 speed preset: faster presets use less CPU but make bigger files
    /// [default: veryfast]
    #[arg(short, long, value_enum)]
    pub preset: Option<Preset>,

    /// Record only this part of the selected screen or window, in pixels from
    /// its top-left corner: WIDTHxHEIGHT+X+Y, e.g. 1280x720+100+200
    #[arg(short, long, value_name = "WxH+X+Y")]
    pub region: Option<Region>,

    /// Use the options saved in this profile (command-line options win)
    #[arg(short = 'P', long, value_name = "NAME")]
    pub profile: Option<String>,

    /// Save the options given (and those of --profile) as this profile,
    /// replacing it if it exists, then record
    #[arg(long, value_name = "NAME")]
    pub save_profile: Option<String>,

    /// List the saved profiles and exit
    #[arg(long, exclusive = true)]
    pub list_profiles: bool,
}

/// Options of this recording, once the profile and the command line are combined.
pub struct Settings {
    pub output: Option<PathBuf>,
    pub quality: u8,
    pub preset: Preset,
    pub region: Option<Region>,
}

impl Args {
    /// Combines the selected profile with the command line, and saves the
    /// result as a profile if `--save-profile` is given.
    pub fn settings(self) -> Result<Settings> {
        self.settings_with(&Profiles::default_path())
    }

    /// Same as [`Args::settings`], with the profiles file at `path`.
    fn settings_with(self, path: &Path) -> Result<Settings> {
        let command_line = Profile {
            output: self.output,
            quality: self.quality,
            preset: self.preset,
            region: self.region,
        };

        // The profiles file is only read when needed: a broken file must not
        // prevent recording without profiles
        let mut combined = command_line;
        if self.profile.is_some() || self.save_profile.is_some() {
            let mut profiles = Profiles::load(path)?;
            if let Some(name) = &self.profile {
                combined = profiles.get(name)?.clone().overridden_by(combined);
            }
            combined.output = combined.output.map(expand_home);

            if let Some(name) = self.save_profile {
                let mut saved = combined.clone();
                // Absolute, so the profile works from any directory
                saved.output = saved
                    .output
                    .map(|output| std::path::absolute(&output).unwrap_or(output));
                profiles.profiles.insert(name.clone(), saved);
                profiles.save(path)?;
                println!("Profile '{name}' saved in {}", path.display());
            }
        }

        Ok(Settings {
            output: combined.output,
            quality: combined.quality.unwrap_or(DEFAULT_QUALITY),
            preset: combined.preset.unwrap_or(DEFAULT_PRESET),
            region: combined.region,
        })
    }
}

/// Prints the saved profiles (`--list-profiles`).
pub fn list_profiles() -> Result<()> {
    let path = Profiles::default_path();
    let profiles = Profiles::load(&path)?;
    if profiles.profiles.is_empty() {
        println!("No profiles saved yet (they go in {}).", path.display());
        return Ok(());
    }
    println!("Profiles in {}:", path.display());
    for (name, profile) in &profiles.profiles {
        println!("  {name}: {profile}");
    }
    Ok(())
}

/// x264 speed presets, from fastest to slowest.
#[derive(Clone, Copy, Debug, PartialEq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(command_line: &[&str]) -> Args {
        Args::try_parse_from(std::iter::once("kapt").chain(command_line.iter().copied())).unwrap()
    }

    /// A profiles file path in a fresh temporary directory.
    fn temp_profiles(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kapt-test-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("profiles.toml")
    }

    #[test]
    fn defaults_without_options() {
        let settings = args(&[]).settings_with(&temp_profiles("defaults")).unwrap();
        assert_eq!(settings.quality, DEFAULT_QUALITY);
        assert_eq!(settings.preset, DEFAULT_PRESET);
        assert_eq!(settings.region, None);
        assert_eq!(settings.output, None);
    }

    #[test]
    fn saved_profile_is_used_and_overridden() {
        let path = temp_profiles("save");
        args(&[
            "-r",
            "1280x720+100+200",
            "-q",
            "23",
            "-o",
            "/tmp/demos",
            "--save-profile",
            "demo",
        ])
        .settings_with(&path)
        .unwrap();

        let settings = args(&["-P", "demo", "-q", "30"])
            .settings_with(&path)
            .unwrap();
        assert_eq!(settings.region, Some("1280x720+100+200".parse().unwrap()));
        assert_eq!(settings.quality, 30);
        assert_eq!(settings.preset, DEFAULT_PRESET);
        assert_eq!(settings.output, Some(PathBuf::from("/tmp/demos")));
    }

    #[test]
    fn saved_output_is_absolute() {
        let path = temp_profiles("absolute");
        args(&["-o", "videos", "--save-profile", "rel"])
            .settings_with(&path)
            .unwrap();
        let saved = Profiles::load(&path).unwrap();
        let output = saved.get("rel").unwrap().output.clone().unwrap();
        assert!(output.is_absolute(), "{}", output.display());
    }

    #[test]
    fn list_profiles_takes_no_other_option() {
        assert!(Args::try_parse_from(["kapt", "--list-profiles", "--save-profile", "x"]).is_err());
    }
}
