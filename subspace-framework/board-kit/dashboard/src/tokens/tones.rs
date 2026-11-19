//! The default glass and flat button tones by theme mode.

/// The selected navigation capsule's glass, as `0xRRGGBBAA`: a dark set for
/// dark themes (One Dark's) and a light set for light themes. Theme metadata
/// may replace each colour with the `glass`, `glass-rim` and `glass-shadow`
/// tones.
pub mod glass {
    /// The translucent fill: a whisper of white over a dark title bar.
    pub const DARK_FILL: u32 = 0xffffff1a;
    /// The specular rim along the capsule's inner top edge.
    pub const DARK_RIM: u32 = 0xffffff40;
    /// The soft shadow that lifts the capsule off the bar.
    pub const DARK_SHADOW: u32 = 0x00000080;
    pub const LIGHT_FILL: u32 = 0xffffffb8;
    pub const LIGHT_RIM: u32 = 0xffffffff;
    pub const LIGHT_SHADOW: u32 = 0x0000002e;
}

/// Flat buttons' feedback fills. The highlight under the pointer is a white
/// wash on dark themes and a black one on light themes, as `0xRRGGBBAA`; a
/// latched button's persistent fill is the accent at this opacity. Theme
/// metadata may replace each with the `button-hover` and `button-on` tones.
pub mod flat {
    pub const DARK_HOVER: u32 = 0xffffff17;
    pub const LIGHT_HOVER: u32 = 0x00000012;
    pub const DARK_ON: f32 = 0.24;
    pub const LIGHT_ON: f32 = 0.16;
}
