//! Capture region (`--region WxH+X+Y`) and the crop it gives on each frame,
//! kept free of any pipeline access so it can be unit tested.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A rectangle in source pixels, from the top-left corner of the selected
/// screen or window. Written `WxH+X+Y`, the usual X11 geometry notation,
/// also in the profiles file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Region {
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
}

impl Region {
    /// The whole frame.
    pub fn full(width: u32, height: u32) -> Self {
        Region {
            width,
            height,
            x: 0,
            y: 0,
        }
    }

    /// The part of the region inside a `width`×`height` frame, or `None` if
    /// less than 2×2 pixels of it are visible (a smaller frame, e.g. a window
    /// made smaller than the region).
    pub fn clamp(self, width: u32, height: u32) -> Option<Region> {
        let right = self.x.saturating_add(self.width).min(width);
        let bottom = self.y.saturating_add(self.height).min(height);
        let clamped = Region {
            width: right.saturating_sub(self.x),
            height: bottom.saturating_sub(self.y),
            ..self
        };
        (clamped.width >= 2 && clamped.height >= 2).then_some(clamped)
    }
}

impl FromStr for Region {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid =
            || format!("invalid region '{text}': expected WxH+X+Y, e.g. 1280x720+100+200");
        let (size, position) = text.split_once('+').ok_or_else(invalid)?;
        let (width, height) = size.split_once('x').ok_or_else(invalid)?;
        let (x, y) = position.split_once('+').ok_or_else(invalid)?;
        let number = |value: &str| value.trim().parse::<u32>().map_err(|_| invalid());
        let region = Region {
            width: number(width)?,
            height: number(height)?,
            x: number(x)?,
            y: number(y)?,
        };
        if region.width < 2 || region.height < 2 {
            return Err(format!(
                "invalid region '{text}': it must be at least 2x2 pixels"
            ));
        }
        Ok(region)
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}x{}+{}+{}", self.width, self.height, self.x, self.y)
    }
}

impl TryFrom<String> for Region {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Region> for String {
    fn from(region: Region) -> Self {
        region.to_string()
    }
}

/// Pixels to remove from each side of a frame (the `videocrop` properties).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Crop {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    /// Size of the frame once cropped.
    pub width: u32,
    pub height: u32,
}

impl Crop {
    /// Crop keeping `region` (which must lie inside the `width`×`height`
    /// frame, see [`Region::clamp`]), trimmed to an even size as 4:2:0
    /// encoding requires (one chroma sample per 2×2 pixels).
    pub fn new(width: u32, height: u32, region: Region) -> Self {
        let kept_width = region.width - region.width % 2;
        let kept_height = region.height - region.height % 2;
        Crop {
            left: region.x,
            top: region.y,
            right: width - region.x - kept_width,
            bottom: height - region.y - kept_height,
            width: kept_width,
            height: kept_height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(width: u32, height: u32, x: u32, y: u32) -> Region {
        Region {
            width,
            height,
            x,
            y,
        }
    }

    #[test]
    fn parses_and_prints_geometry() {
        let parsed: Region = "1280x720+100+200".parse().unwrap();
        assert_eq!(parsed, region(1280, 720, 100, 200));
        assert_eq!(parsed.to_string(), "1280x720+100+200");
    }

    #[test]
    fn rejects_malformed_geometry() {
        for text in [
            "",
            "1280x720",
            "1280x720+100",
            "1280+720+1+2",
            "ax720+0+0",
            "-5x2+0+0",
        ] {
            assert!(text.parse::<Region>().is_err(), "{text} accepted");
        }
    }

    #[test]
    fn rejects_regions_smaller_than_2x2() {
        assert!("1x720+0+0".parse::<Region>().is_err());
        assert!("1280x0+0+0".parse::<Region>().is_err());
    }

    #[test]
    fn region_inside_frame_is_kept() {
        let inside = region(1280, 720, 100, 200);
        assert_eq!(inside.clamp(1920, 1080), Some(inside));
    }

    #[test]
    fn region_overflowing_frame_is_clamped() {
        let overflowing = region(1280, 720, 1000, 500);
        assert_eq!(
            overflowing.clamp(1920, 1080),
            Some(region(920, 580, 1000, 500))
        );
    }

    #[test]
    fn region_outside_frame_is_rejected() {
        assert_eq!(region(100, 100, 2000, 0).clamp(1920, 1080), None);
        // Only one pixel column visible
        assert_eq!(region(100, 100, 1919, 0).clamp(1920, 1080), None);
    }

    #[test]
    fn huge_values_do_not_overflow() {
        let huge = region(u32::MAX, u32::MAX, 10, 10);
        assert_eq!(huge.clamp(100, 100), Some(region(90, 90, 10, 10)));
    }

    #[test]
    fn crop_of_full_frame_is_empty() {
        let crop = Crop::new(1920, 1080, Region::full(1920, 1080));
        assert_eq!(
            crop,
            Crop {
                width: 1920,
                height: 1080,
                ..Crop::default()
            }
        );
    }

    #[test]
    fn crop_trims_odd_frame_size() {
        let crop = Crop::new(321, 241, Region::full(321, 241));
        assert_eq!((crop.right, crop.bottom), (1, 1));
        assert_eq!((crop.width, crop.height), (320, 240));
    }

    #[test]
    fn crop_keeps_region_with_even_size() {
        let crop = Crop::new(1920, 1080, region(1281, 721, 100, 200));
        assert_eq!(
            crop,
            Crop {
                left: 100,
                top: 200,
                right: 1920 - 100 - 1280,
                bottom: 1080 - 200 - 720,
                width: 1280,
                height: 720,
            }
        );
    }
}
