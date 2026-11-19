//! A web app served from a folder of built files. Each board's app has a
//! private origin, `subspace://<board>/` (`https://subspace.<board>/` in
//! WebView2), and its own handler serves only that board's folder.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The custom URL scheme web app boards use.
pub(crate) const APP_SCHEME: &str = "subspace";
/// The largest file the board serves; responses are held in memory.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const INDEX: &str = "index.html";

/// HTTP statuses the handler answers with.
const OK: u16 = 200;
const BAD_REQUEST: u16 = 400;
const FORBIDDEN: u16 = 403;
const NOT_FOUND: u16 = 404;
const METHOD_NOT_ALLOWED: u16 = 405;
const TOO_LARGE: u16 = 413;
const READ_FAILED: u16 = 500;

/// The origin of `board`'s app as the page sees it. WebView2 serves custom
/// schemes from an `https://<scheme>.<host>` address.
pub(crate) fn app_origin(board: &str) -> String {
    origin_for(cfg!(windows), board)
}

fn origin_for(windows: bool, board: &str) -> String {
    if windows {
        format!("https://{APP_SCHEME}.{board}")
    } else {
        format!("{APP_SCHEME}://{board}")
    }
}

/// Whether `value` is a page of the app at `origin`.
pub(crate) fn same_origin(value: &str, origin: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default();
    url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && format!("{}://{host}", url.scheme()) == origin
}

/// A file the handler serves, or the HTTP status of why it does not.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Served {
    pub status: u16,
    pub mime: &'static str,
    pub body: Vec<u8>,
}

impl Served {
    fn error(status: u16) -> Self {
        Self {
            status,
            mime: "text/plain; charset=utf-8",
            body: Vec::new(),
        }
    }
}

/// Answer a request for `uri` with `method` from `board`'s folder `root`.
// Only native web views call the handler; keep its confinement tests on Linux.
#[cfg_attr(not(any(target_os = "macos", windows, test)), allow(dead_code))]
pub(crate) fn serve(root: &Path, board: &str, method: &str, uri: &str) -> Served {
    if !matches!(method, "GET" | "HEAD") {
        return Served::error(METHOD_NOT_ALLOWED);
    }
    let file = request_path(board, uri).and_then(|relative| resolve(root, &relative));
    let mut served = file.map_or_else(Served::error, |file| read(&file));
    if method == "HEAD" {
        served.body.clear();
    }
    served
}

/// The file path a request names, relative to the app folder. `None` for
/// another board's address, or a path that could leave the folder.
fn request_path(board: &str, uri: &str) -> Result<PathBuf, u16> {
    let url = url::Url::parse(uri).map_err(|_| BAD_REQUEST)?;
    if url.scheme() != APP_SCHEME || url.host_str() != Some(board) || url.port().is_some() {
        return Err(NOT_FOUND);
    }
    let mut path = PathBuf::new();
    for segment in url.path_segments().into_iter().flatten() {
        let decoded = percent_encoding::percent_decode_str(segment)
            .decode_utf8()
            .map_err(|_| BAD_REQUEST)?;
        if !segment_allowed(&decoded) {
            return Err(NOT_FOUND);
        }
        if !decoded.is_empty() {
            path.push(decoded.as_ref());
        }
    }
    if url.path().ends_with('/') || path.as_os_str().is_empty() {
        path.push(INDEX);
    }
    Ok(path)
}

/// One decoded path segment: a plain name, never `.`, `..`, a separator,
/// a drive or a control character.
fn segment_allowed(segment: &str) -> bool {
    let mut parts = Path::new(segment).components();
    let plain = segment.is_empty()
        || (matches!(parts.next(), Some(Component::Normal(_))) && parts.next().is_none());
    plain && !segment.contains(['/', '\\', ':']) && !segment.chars().any(char::is_control)
}

/// The real file for `relative` inside `root`. A missing path without an
/// extension is a client-side route, so the app's `index.html` answers it.
fn resolve(root: &Path, relative: &Path) -> Result<PathBuf, u16> {
    let root = root.canonicalize().map_err(|_| NOT_FOUND)?;
    let found = match root.join(relative).canonicalize() {
        Ok(found) => found,
        Err(error) if error.kind() == io::ErrorKind::NotFound && relative.extension().is_none() => {
            root.join(INDEX).canonicalize().map_err(|_| NOT_FOUND)?
        }
        Err(_) => return Err(NOT_FOUND),
    };
    if !found.starts_with(&root) {
        return Err(FORBIDDEN);
    }
    Ok(found)
}

fn read(file: &Path) -> Served {
    match fs::metadata(file) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_FILE_BYTES => match fs::read(file) {
            Ok(body) => Served {
                status: OK,
                mime: mime_type(file),
                body,
            },
            Err(_) => Served::error(READ_FAILED),
        },
        Ok(meta) if meta.is_file() => Served::error(TOO_LARGE),
        _ => Served::error(NOT_FOUND),
    }
}

fn mime_type(file: &Path) -> &'static str {
    let extension = file
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("wasm") => "application/wasm",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{origin_for, request_path, same_origin, serve};

    fn app() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("index.html"), "<p>home</p>").unwrap();
        fs::create_dir(root.path().join("assets")).unwrap();
        fs::write(root.path().join("assets/app.js"), "run()").unwrap();
        root
    }

    #[test]
    fn files_index_and_client_routes_are_served() {
        let root = app();
        let home = serve(root.path(), "notes", "GET", "subspace://notes/");
        assert_eq!((home.status, home.mime), (200, "text/html; charset=utf-8"));
        assert_eq!(home.body, b"<p>home</p>");
        let script = serve(
            root.path(),
            "notes",
            "GET",
            "subspace://notes/assets/app.js?v=1",
        );
        assert_eq!(script.body, b"run()");
        assert!(script.mime.starts_with("text/javascript"));
        let route = serve(root.path(), "notes", "GET", "subspace://notes/items/42");
        assert_eq!(route.body, b"<p>home</p>");
        let head = serve(root.path(), "notes", "HEAD", "subspace://notes/");
        assert_eq!((head.status, head.body.len()), (200, 0));
    }

    #[test]
    fn missing_files_other_boards_and_methods_are_refused() {
        let root = app();
        assert_eq!(
            serve(root.path(), "notes", "GET", "subspace://notes/x.png").status,
            404
        );
        assert_eq!(
            serve(root.path(), "notes", "GET", "subspace://other/").status,
            404
        );
        assert_eq!(
            serve(root.path(), "notes", "GET", "https://notes/").status,
            404
        );
        assert_eq!(
            serve(root.path(), "notes", "POST", "subspace://notes/").status,
            405
        );
    }

    #[test]
    fn paths_cannot_leave_the_folder() {
        // The URL parser resolves dot segments at the folder root.
        for uri in [
            "subspace://notes/%2e%2e/secret.txt",
            "subspace://notes/assets/%2E%2E/%2E%2E/secret.txt",
            "subspace://notes/../../secret.txt",
        ] {
            assert_eq!(
                request_path("notes", uri).unwrap(),
                Path::new("secret.txt"),
                "{uri}"
            );
        }
        for uri in [
            "subspace://notes/..%2fsecret.txt",
            "subspace://notes/..%5csecret.txt",
            "subspace://notes/C:%5cWindows",
            "subspace://notes/a%00b",
        ] {
            assert!(request_path("notes", uri).is_err(), "{uri}");
        }
        assert_eq!(
            request_path("notes", "subspace://notes/a%20b/c.js").unwrap(),
            Path::new("a b").join("c.js")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_folder_is_refused() {
        let root = app();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("out")).unwrap();
        let served = serve(
            root.path(),
            "notes",
            "GET",
            "subspace://notes/out/secret.txt",
        );
        assert_eq!(served.status, 403);
        assert!(served.body.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_route_fallback_cannot_follow_an_index_link_outside_the_folder() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), "private").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("index.html")).unwrap();
        let served = serve(
            root.path(),
            "notes",
            "GET",
            "subspace://notes/missing/route",
        );
        assert_eq!(served.status, 403);
        assert!(served.body.is_empty());
    }

    #[test]
    fn origins_match_each_engine() {
        assert_eq!(origin_for(false, "notes"), "subspace://notes");
        assert_eq!(origin_for(true, "notes"), "https://subspace.notes");
        assert!(same_origin("subspace://notes/a?b", "subspace://notes"));
        assert!(same_origin(
            "https://subspace.notes/",
            "https://subspace.notes"
        ));
        assert!(!same_origin("subspace://other/", "subspace://notes"));
        assert!(!same_origin(
            "https://subspace.notes:8443/",
            "https://subspace.notes"
        ));
        assert!(!same_origin("not a url", "subspace://notes"));
    }
}
