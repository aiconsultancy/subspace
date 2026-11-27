//! The local file system: a chosen file read within the upload limit, and
//! where, and under which name, a saved attachment is offered.

use subspace_api_client::uploads::{AttachmentProtocol, UploadError, file_mime};

/// A chosen file's name, type and bytes. Only as much as the largest upload
/// of its type allows, and one byte more, is read, so a larger file is
/// refused before it is held in memory.
pub(super) fn read_chosen(
    path: &std::path::Path,
    protocol: AttachmentProtocol,
) -> Result<(String, String, Vec<u8>), UploadError> {
    let invalid = || protocol.invalid_file();
    let mime = file_mime(path).ok_or_else(invalid)?;
    let name = path
        .file_name()
        .ok_or_else(invalid)?
        .to_string_lossy()
        .into_owned();
    let file = std::fs::File::open(path).map_err(|_| invalid())?;
    let bytes = read_at_most(file, protocol.max_bytes(mime)).ok_or_else(invalid)?;
    Ok((name, mime.to_owned(), bytes))
}

/// At most `limit` bytes from `reader`, or `None` when it holds more.
fn read_at_most(reader: impl std::io::Read, limit: usize) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let bound = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    let mut bytes = Vec::new();
    reader.take(bound).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= limit).then_some(bytes)
}

/// Where a saved attachment goes unless the person chooses otherwise: the
/// Downloads folder, else the home folder, rather than wherever the app
/// happened to start.
pub(super) fn save_folder() -> std::path::PathBuf {
    std::env::home_dir().map_or_else(
        || std::path::PathBuf::from("."),
        |home| {
            let downloads = home.join("Downloads");
            if downloads.is_dir() { downloads } else { home }
        },
    )
}

/// The name a saved attachment is offered under. The name comes from the
/// service, so it is never a path, a hidden file, a disguised extension or
/// a name some system cannot save: format and bidirectional characters go,
/// separators, control characters and the characters Windows forbids
/// become `_`, leading dots and spaces and trailing dots and spaces go, and
/// a Windows device name gains a leading `_`.
pub(super) fn suggested_name(name: &str) -> String {
    let visible = visible(name);
    outside_device_names(plain(&visible))
}

/// `name` without format characters, stripped as uploads strip them. Should
/// they not be told apart, no name the service gave is trusted.
fn visible(name: &str) -> String {
    subspace_api_client::uploads::without_format_characters(name)
        .unwrap_or_else(|| "attachment".to_owned())
}

/// `name` with what no file name may hold replaced, and the dots and spaces
/// that would hide it or that Windows drops trimmed away.
fn plain(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            let forbidden = c.is_control()
                || matches!(
                    c,
                    '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*' | '\u{2028}' | '\u{2029}'
                );
            if forbidden { '_' } else { c }
        })
        .collect();
    let trimmed = cleaned
        .trim_start_matches(['.', ' '])
        .trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "attachment".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// `name`, kept clear of the device names Windows reserves, alone or
/// before any extension (`NUL.txt` is `NUL`), in any case.
fn outside_device_names(name: String) -> String {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    // COM and LPT ports take one digit, or a superscript one, two or three.
    let numbered = |prefix| {
        stem.strip_prefix(prefix).is_some_and(|number: &str| {
            let mut chars = number.chars();
            matches!(
                (chars.next(), chars.next()),
                (Some('0'..='9' | '\u{B9}' | '\u{B2}' | '\u{B3}'), None)
            )
        })
    };
    let device = matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || numbered("COM")
        || numbered("LPT");
    if device { format!("_{name}") } else { name }
}

#[cfg(test)]
mod tests {
    use super::{AttachmentProtocol, UploadError, read_at_most, read_chosen, suggested_name};

    /// Counts the bytes read through it.
    struct Counted<R> {
        inner: R,
        read: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl<R: std::io::Read> std::io::Read for Counted<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let count = self.inner.read(buf)?;
            self.read.set(self.read.get() + count);
            Ok(count)
        }
    }

    #[test]
    fn a_chosen_file_is_read_only_as_far_as_the_limit() {
        let read = std::rc::Rc::new(std::cell::Cell::new(0));
        // An endless source stands in for a file of many gigabytes.
        let huge = Counted {
            inner: std::io::repeat(b'a'),
            read: std::rc::Rc::clone(&read),
        };
        assert_eq!(read_at_most(huge, 1024), None);
        assert_eq!(read.get(), 1025, "one byte past the limit, no more");
        assert_eq!(
            read_at_most(&[b'a'; 1024][..], 1024),
            Some(vec![b'a'; 1024])
        );
        assert_eq!(read_at_most(&[][..], 1024), Some(Vec::new()));
    }

    #[test]
    fn a_file_larger_than_an_upload_is_refused_before_it_is_read() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("huge.txt");
        let file = std::fs::File::create(&path).unwrap();
        // Sparse where the file system allows: larger than any upload.
        file.set_len(64 * 1024 * 1024).unwrap();
        let protocol = AttachmentProtocol::SubspaceFiles;
        assert_eq!(
            read_chosen(&path, protocol),
            Err(UploadError::UnsupportedFile)
        );
        assert_eq!(
            read_chosen(&path, AttachmentProtocol::NettStandalone),
            Err(UploadError::InvalidImage)
        );
        let path = folder.path().join("small.txt");
        std::fs::write(&path, b"Small enough.\n").unwrap();
        let (name, mime, bytes) = read_chosen(&path, protocol).unwrap();
        assert_eq!((name.as_str(), mime.as_str()), ("small.txt", "text/plain"));
        assert_eq!(bytes, b"Small enough.\n");
    }

    #[test]
    fn a_saved_attachment_is_offered_under_a_plain_file_name() {
        assert_eq!(suggested_name("notes.txt"), "notes.txt");
        assert_eq!(suggested_name(".zshrc"), "zshrc");
        assert_eq!(suggested_name("../../.ssh/config"), "_.._.ssh_config");
        assert_eq!(suggested_name("a\\b:c\u{7}.pdf"), "a_b_c_.pdf");
        assert_eq!(suggested_name("report.pdf. "), "report.pdf");
        assert_eq!(suggested_name(" ... "), "attachment");
    }

    #[test]
    fn a_saved_attachment_never_disguises_its_extension() {
        // Shown right to left, this reads as "invoiceexe.pdf".
        assert_eq!(suggested_name("invoice\u{202E}fdp.exe"), "invoicefdp.exe");
        assert_eq!(suggested_name("\u{2067}notes\u{2069}.txt"), "notes.txt");
        assert_eq!(suggested_name("a\u{200B}b\u{200F}.txt"), "ab.txt");
        assert_eq!(suggested_name("\u{FEFF}plan.md"), "plan.md");
        // A format character cannot shield a trailing dot either.
        assert_eq!(suggested_name("report.pdf.\u{200B}"), "report.pdf");
        assert_eq!(suggested_name("\u{202E}\u{202C}"), "attachment");
        assert_eq!(suggested_name("line\u{2028}break.txt"), "line_break.txt");
    }

    #[test]
    fn a_saved_attachment_is_named_so_windows_can_save_it() {
        assert_eq!(suggested_name("a<b>c\"d|e?f*g.txt"), "a_b_c_d_e_f_g.txt");
        for (reserved, offered) in [
            ("NUL.txt", "_NUL.txt"),
            ("con", "_con"),
            ("Aux.tar.gz", "_Aux.tar.gz"),
            ("com1.log", "_com1.log"),
            ("LPT9", "_LPT9"),
            ("COM\u{B9}.txt", "_COM\u{B9}.txt"),
            ("conin$", "_conin$"),
            ("nul .txt", "_nul .txt"),
        ] {
            assert_eq!(suggested_name(reserved), offered, "{reserved}");
        }
        for ordinary in [
            "console.txt",
            "com10.txt",
            "nullable.md",
            "lpt.txt",
            "prn-notes.pdf",
        ] {
            assert_eq!(suggested_name(ordinary), ordinary);
        }
    }
}
