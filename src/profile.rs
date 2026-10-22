//! Named profiles: sets of options saved in `~/.config/kapt/profiles.toml`.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use gstreamer::glib;
use serde::{Deserialize, Serialize};

use crate::cli::Preset;
use crate::region::Region;

/// Options saved in a profile. Only the options given explicitly are stored,
/// so a profile follows future changes of the defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<Preset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
}

impl Profile {
    /// This profile with the options set in `over` replacing its own
    /// (command-line options win over the profile).
    pub fn overridden_by(self, over: Profile) -> Profile {
        Profile {
            output: over.output.or(self.output),
            quality: over.quality.or(self.quality),
            preset: over.preset.or(self.preset),
            region: over.region.or(self.region),
        }
    }
}

/// One line for `--list-profiles`, e.g. `region=1280x720+100+200 quality=23`.
impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut fields = Vec::new();
        if let Some(region) = self.region {
            fields.push(format!("region={region}"));
        }
        if let Some(quality) = self.quality {
            fields.push(format!("quality={quality}"));
        }
        if let Some(preset) = self.preset {
            fields.push(format!("preset={}", preset.as_str()));
        }
        if let Some(output) = &self.output {
            fields.push(format!("output={}", output.display()));
        }
        if fields.is_empty() {
            return f.write_str("(no options)");
        }
        f.write_str(&fields.join(" "))
    }
}

/// Content of the profiles file.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

impl Profiles {
    /// Default location: `$XDG_CONFIG_HOME/kapt/profiles.toml`.
    pub fn default_path() -> PathBuf {
        glib::user_config_dir().join("kapt").join("profiles.toml")
    }

    /// Reads the profiles file; a missing file means no profiles yet.
    pub fn load(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(err).with_context(|| format!("cannot read {}", path.display())),
        };
        Self::parse(&text).with_context(|| format!("invalid profiles file {}", path.display()))
    }

    fn parse(text: &str) -> Result<Self> {
        let profiles: Self = toml::from_str(text)?;
        for (name, profile) in &profiles.profiles {
            if profile.quality.is_some_and(|quality| quality > 51) {
                bail!("profile '{name}': quality must be between 0 and 51");
            }
        }
        Ok(profiles)
    }

    /// Writes the profiles file, creating its directory if needed.
    /// Comments added by hand are not kept.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
        }
        let text = toml::to_string_pretty(self).context("cannot serialize the profiles")?;
        std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))
    }

    pub fn get(&self, name: &str) -> Result<&Profile> {
        self.profiles.get(name).with_context(|| {
            let known: Vec<_> = self.profiles.keys().map(String::as_str).collect();
            if known.is_empty() {
                format!("unknown profile '{name}': no profiles saved yet")
            } else {
                format!("unknown profile '{name}' (known: {})", known.join(", "))
            }
        })
    }
}

/// Expands a leading `~/` (allowed in the hand-edited profiles file, where
/// no shell does it) to the home directory.
pub fn expand_home(path: PathBuf) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(rest) => glib::home_dir().join(rest),
        Err(_) => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
[profiles.demo]
region = "1280x720+100+200"
quality = 23
preset = "fast"
output = "~/Videos/demos"

[profiles.quick]
preset = "ultrafast"
"#;

    #[test]
    fn parses_profiles_file() {
        let profiles = Profiles::parse(EXAMPLE).unwrap();
        assert_eq!(
            profiles.get("demo").unwrap(),
            &Profile {
                output: Some(PathBuf::from("~/Videos/demos")),
                quality: Some(23),
                preset: Some(Preset::Fast),
                region: Some("1280x720+100+200".parse().unwrap()),
            }
        );
        assert_eq!(
            profiles.get("quick").unwrap(),
            &Profile {
                preset: Some(Preset::Ultrafast),
                ..Profile::default()
            }
        );
    }

    #[test]
    fn empty_file_has_no_profiles() {
        assert_eq!(Profiles::parse("").unwrap(), Profiles::default());
    }

    #[test]
    fn rejects_invalid_values() {
        assert!(Profiles::parse("[profiles.a]\nregion = \"1280x720\"").is_err());
        assert!(Profiles::parse("[profiles.a]\nquality = 60").is_err());
        assert!(Profiles::parse("[profiles.a]\npreset = \"turbo\"").is_err());
        // A typo must not be silently ignored
        assert!(Profiles::parse("[profiles.a]\nqualty = 20").is_err());
    }

    #[test]
    fn unknown_profile_lists_known_ones() {
        let profiles = Profiles::parse(EXAMPLE).unwrap();
        let err = profiles.get("dmeo").unwrap_err().to_string();
        assert!(err.contains("demo, quick"), "{err}");
    }

    #[test]
    fn save_then_load_gives_the_same_profiles() {
        let profiles = Profiles::parse(EXAMPLE).unwrap();
        let text = toml::to_string_pretty(&profiles).unwrap();
        assert_eq!(Profiles::parse(&text).unwrap(), profiles);
        // Unset options are not written
        assert!(!text.contains("quick]\nquality"), "{text}");
    }

    #[test]
    fn command_line_wins_over_profile() {
        let profile = Profile {
            quality: Some(23),
            preset: Some(Preset::Fast),
            ..Profile::default()
        };
        let command_line = Profile {
            quality: Some(18),
            ..Profile::default()
        };
        assert_eq!(
            profile.overridden_by(command_line),
            Profile {
                quality: Some(18),
                preset: Some(Preset::Fast),
                ..Profile::default()
            }
        );
    }

    #[test]
    fn expands_home_only_at_start() {
        let home = glib::home_dir();
        assert_eq!(expand_home(PathBuf::from("~/Videos")), home.join("Videos"));
        assert_eq!(
            expand_home(PathBuf::from("/tmp/~/x")),
            PathBuf::from("/tmp/~/x")
        );
    }
}
