//! Terminal.app saves its default profile name and a keyed NSFont archive.

use std::path::{Path, PathBuf};
use std::sync::Once;

use gpui_kit::{FontStyle, FontWeight};
use objc2::{ClassType as _, MainThreadMarker, rc::Retained, runtime::AnyObject};
use objc2_app_kit::{NSFont, NSFontManager, NSFontTraitMask};
use objc2_core_text::{CTFontManagerRegisterFontsForURL, CTFontManagerScope};
use objc2_foundation::{
    NSData, NSDictionary, NSKeyedUnarchiver, NSPropertyListMutabilityOptions,
    NSPropertyListSerialization, NSString, NSURL,
};

use crate::style::ProfileFont;

const TERMINAL_PREFERENCES: &str = "Library/Preferences/com.apple.Terminal.plist";
const RESOURCES: &str = "/System/Applications/Utilities/Terminal.app/Contents/Resources";
const BUNDLED_PROFILES: &str = "Initial Settings";
const DEFAULT_PROFILE: &str = "Default Window Settings";
const PROFILES: &str = "Window Settings";
type Dictionary = NSDictionary;

pub(in crate::settings) fn load() -> ProfileFont {
    let preferences = std::env::var_os("HOME")
        .map(PathBuf::from)
        .and_then(|home| read_plist(&home.join(TERMINAL_PREFERENCES)));
    let profile = preferences
        .as_deref()
        .and_then(|preferences| lookup::<NSString>(preferences, DEFAULT_PROFILE));
    let name = profile
        .as_deref()
        .map_or_else(built_in_default, ToString::to_string);
    preferences
        .as_deref()
        .and_then(|preferences| saved_font(preferences, &name))
        .or_else(|| built_in_font(&name))
        .unwrap_or_default()
}

fn saved_font(preferences: &Dictionary, name: &str) -> Option<ProfileFont> {
    let profiles = lookup::<Dictionary>(preferences, PROFILES)?;
    let profile = lookup::<Dictionary>(&profiles, name)?;
    font(&profile)
}

fn built_in_default() -> String {
    let path = Path::new(RESOURCES)
        .join(BUNDLED_PROFILES)
        .join("settings.plist");
    read_plist(&path)
        .and_then(|settings| lookup::<NSString>(&settings, "Default Settings"))
        .map_or_else(|| "Basic".to_owned(), |name| name.to_string())
}

fn built_in_font(name: &str) -> Option<ProfileFont> {
    let path = Path::new(name);
    if path.file_name().and_then(|name| name.to_str()) != Some(name) {
        return None;
    }
    let profile = read_plist(
        &Path::new(RESOURCES)
            .join(BUNDLED_PROFILES)
            .join(format!("{name}.terminal")),
    )?;
    font(&profile)
}

// qual:allow(unsafe) reason: "The immutable plist decoder accepts a null format out parameter and returns a checked native object."
fn read_plist(path: &Path) -> Option<Retained<Dictionary>> {
    let data = NSData::from_vec(std::fs::read(path).ok()?);
    // SAFETY: no format out parameter is needed. The plist is read immutably.
    let value = unsafe {
        NSPropertyListSerialization::propertyListWithData_options_format_error(
            &data,
            NSPropertyListMutabilityOptions::Immutable,
            std::ptr::null_mut(),
        )
    }
    .ok()?;
    checked(value)
}

fn lookup<T: objc2::DowncastTarget>(dict: &Dictionary, key: &str) -> Option<Retained<T>> {
    checked(dict.objectForKey(NSString::from_str(key).as_ref())?)
}

// qual:allow(coupling, deh) reason: "Native property lists and archives return dynamic objects. Validate their actual class before reading typed values."
fn checked<T: objc2::DowncastTarget>(value: Retained<AnyObject>) -> Option<Retained<T>> {
    value.downcast().ok()
}

// qual:allow(unsafe) reason: "Secure AppKit decoding is restricted to NSFont and verifies the returned native class."
fn font(profile: &Dictionary) -> Option<ProfileFont> {
    let archive = lookup::<NSData>(profile, "Font")?;
    // SAFETY: secure decoding is restricted to NSFont and verifies the root type.
    let value = unsafe {
        NSKeyedUnarchiver::unarchivedObjectOfClass_fromData_error(NSFont::class(), &archive)
    }
    .ok()?;
    let font: Retained<NSFont> = checked(value)?;
    let size = font.pointSize() as f32;
    let manager = NSFontManager::sharedFontManager(MainThreadMarker::new()?);
    Some(ProfileFont {
        family: font.familyName().map(|family| family.to_string()),
        size: (size.is_finite() && size > 0.0).then_some(size),
        weight: font_weight(manager.weightOfFont(&font)),
        style: if manager
            .traitsOfFont(&font)
            .contains(NSFontTraitMask::ItalicFontMask)
        {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        },
    })
}

/// Convert AppKit's weight ranks to the weights GPUI requests by family.
fn font_weight(rank: isize) -> FontWeight {
    match rank {
        0..=2 => FontWeight::THIN,
        3 => FontWeight::EXTRA_LIGHT,
        4 => FontWeight::LIGHT,
        6 => FontWeight::MEDIUM,
        7 | 8 => FontWeight::SEMIBOLD,
        9 => FontWeight::BOLD,
        10 | 11 => FontWeight::EXTRA_BOLD,
        12.. => FontWeight::BLACK,
        _ => FontWeight::NORMAL,
    }
}

/// Terminal.app ships several SF Mono faces that AppKit cannot resolve
/// outside Terminal. Register only the system's copies, for this process.
pub(in crate::settings) fn register() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let directory = Path::new(RESOURCES).join("Fonts");
        for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
            let path = entry.path();
            if matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("otf" | "ttf")
            ) {
                register_font(&path);
            }
        }
    });
}

// qual:allow(unsafe) reason: "Scoped Core Text registration uses a native file URL and a null error pointer."
fn register_font(path: &Path) {
    if let Some(url) = NSURL::from_file_path(path) {
        // SAFETY: NSURL bridges to CFURL; registration needs no error out parameter.
        unsafe {
            CTFontManagerRegisterFontsForURL(
                url.as_ref(),
                CTFontManagerScope::Process,
                std::ptr::null_mut(),
            )
        };
    }
}
