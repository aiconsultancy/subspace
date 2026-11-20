//! The local file system: a chosen file read within the upload limit, and
//! where, and under which name, a saved attachment is offered.

/// A chosen file's name, type and bytes. Only as much as the largest upload
/// of its type allows, and one byte more, is read, so a larger file is
/// refused before it is held in memory.
pub(super) fn read_chosen(path: &std::path::Path) -> Option<(String, String, Vec<u8>)> {
    let mime = subspace_api_client::uploads::file_mime(path)?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    let file = std::fs::File::open(path).ok()?;
    let bytes = read_at_most(file, subspace_api_client::uploads::max_bytes(mime))?;
    Some((name, mime.to_owned(), bytes))
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
/// service, so it is never a path, a hidden file or a run of control
/// characters: separators and control characters become `_`, and leading
/// dots and spaces and trailing dots and spaces go.
pub(super) fn suggested_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':') {
                '_'
            } else {
                c
            }
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

#[cfg(test)]
mod tests {
    use super::{read_at_most, read_chosen, suggested_name};

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
        assert!(read_chosen(&path).is_none());
        let path = folder.path().join("small.txt");
        std::fs::write(&path, b"Small enough.\n").unwrap();
        let (name, mime, bytes) = read_chosen(&path).unwrap();
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
}
