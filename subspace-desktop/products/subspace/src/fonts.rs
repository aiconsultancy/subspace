//! The fonts Subspace embeds and registers at startup: Inter for interface
//! text and headings, and JetBrains Mono for code, logs, keys and counts.
//! Both are under the SIL Open Font License 1.1; their licences and
//! provenance are in `licenses/fonts/`.

use subspace_framework::shell::BundledFont;

const INTER: &str = "Inter";
const JETBRAINS_MONO: &str = "JetBrains Mono";

/// Static faces for the weights the interface draws (400, 500, 600 and 700)
/// and the italics Markdown, chat and syntax colours use (400 and 700).
pub(crate) fn bundled() -> &'static [BundledFont] {
    const FONTS: &[BundledFont] = &[
        BundledFont::new(INTER, include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        BundledFont::new(INTER, include_bytes!("../assets/fonts/Inter-Italic.ttf")),
        BundledFont::new(INTER, include_bytes!("../assets/fonts/Inter-Medium.ttf")),
        BundledFont::new(INTER, include_bytes!("../assets/fonts/Inter-SemiBold.ttf")),
        BundledFont::new(INTER, include_bytes!("../assets/fonts/Inter-Bold.ttf")),
        BundledFont::new(
            INTER,
            include_bytes!("../assets/fonts/Inter-BoldItalic.ttf"),
        ),
        BundledFont::new(
            JETBRAINS_MONO,
            include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
        ),
        BundledFont::new(
            JETBRAINS_MONO,
            include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf"),
        ),
        BundledFont::new(
            JETBRAINS_MONO,
            include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
        ),
        BundledFont::new(
            JETBRAINS_MONO,
            include_bytes!("../assets/fonts/JetBrainsMono-SemiBold.ttf"),
        ),
        BundledFont::new(
            JETBRAINS_MONO,
            include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
        ),
        BundledFont::new(
            JETBRAINS_MONO,
            include_bytes!("../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
        ),
    ];
    FONTS
}

/// The embedded files, checked against their own OpenType tables so a
/// renamed or replaced file cannot register under the wrong family.
#[cfg(test)]
mod tests {
    use super::{INTER, JETBRAINS_MONO, bundled};

    /// The repository's large-file hook limit, per file.
    const FILE_LIMIT: usize = 1024 * 1024;
    /// What the interface draws: (weight, italic).
    const STYLES: [(u16, bool); 6] = [
        (400, false),
        (400, true),
        (500, false),
        (600, false),
        (700, false),
        (700, true),
    ];

    fn u16_at(bytes: &[u8], offset: usize) -> u16 {
        u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
    }

    fn u32_at(bytes: &[u8], offset: usize) -> usize {
        let word = bytes[offset..offset + 4].try_into().unwrap();
        usize::try_from(u32::from_be_bytes(word)).unwrap()
    }

    /// The offset of the table `tag`, from the font's table directory.
    fn table(bytes: &[u8], tag: &[u8; 4]) -> Option<usize> {
        let count = usize::from(u16_at(bytes, 4));
        (0..count)
            .map(|index| 12 + 16 * index)
            .find(|record| bytes[*record..*record + 4] == *tag)
            .map(|record| u32_at(bytes, record + 8))
    }

    /// The Windows, US English name `id`, decoded from UTF-16BE.
    fn name(bytes: &[u8], id: u16) -> Option<String> {
        let table = table(bytes, b"name").unwrap();
        let count = usize::from(u16_at(bytes, table + 2));
        let strings = table + usize::from(u16_at(bytes, table + 4));
        let record = (0..count)
            .map(|index| table + 6 + 12 * index)
            .find(|record| {
                u16_at(bytes, *record) == 3
                    && u16_at(bytes, record + 4) == 0x409
                    && u16_at(bytes, record + 6) == id
            })?;
        let start = strings + usize::from(u16_at(bytes, record + 10));
        let end = start + usize::from(u16_at(bytes, record + 8));
        let units: Vec<u16> = bytes[start..end]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect();
        Some(String::from_utf16(&units).unwrap())
    }

    /// The typographic family the text systems group the face under.
    fn family(bytes: &[u8]) -> String {
        name(bytes, 16).or_else(|| name(bytes, 1)).unwrap()
    }

    /// The OS/2 weight class, and whether `fsSelection` marks it italic.
    fn style(bytes: &[u8]) -> (u16, bool) {
        let os2 = table(bytes, b"OS/2").unwrap();
        (u16_at(bytes, os2 + 4), u16_at(bytes, os2 + 62) & 1 == 1)
    }

    #[test]
    fn every_face_is_a_static_truetype_font_of_its_declared_family() {
        for font in bundled() {
            assert_eq!(font.bytes[..4], [0, 1, 0, 0], "{}: TrueType", font.family);
            assert_eq!(table(font.bytes, b"fvar"), None, "{}: static", font.family);
            assert_eq!(family(font.bytes), font.family);
        }
    }

    #[test]
    fn each_family_has_the_weights_and_italics_the_interface_draws() {
        for family in [INTER, JETBRAINS_MONO] {
            let mut styles: Vec<_> = bundled()
                .iter()
                .filter(|font| font.family == family)
                .map(|font| style(font.bytes))
                .collect();
            styles.sort_unstable();
            assert_eq!(styles, STYLES, "{family}");
        }
    }

    #[test]
    fn every_face_stays_under_the_repository_file_limit() {
        for font in bundled() {
            assert!(font.bytes.len() < FILE_LIMIT, "{}", font.family);
        }
        let total: usize = bundled().iter().map(|font| font.bytes.len()).sum();
        assert!(total < 5 * FILE_LIMIT, "{total} bytes embedded");
    }

    /// The names Twilight Two asks for, as the shell's startup registers them.
    #[test]
    fn the_shell_counts_both_families_as_installed() {
        assert_eq!(
            subspace_framework::shell::test_support::registered_font_families(bundled()),
            Ok(vec![INTER.to_owned(), JETBRAINS_MONO.to_owned()])
        );
    }
}
