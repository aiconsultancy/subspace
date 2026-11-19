//! Release asset names, bounded reads, checksum verification, archive
//! extraction and the `product.json` identity check (REQ-05).

use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subspace_product::UpdateSource;

/// Largest asset or archive entry this reads (512 MiB).
pub(crate) const MAX_DOWNLOAD: u64 = 512 << 20;
/// Largest release-metadata response this reads (4 MiB).
pub(crate) const MAX_METADATA: u64 = 4 << 20;

const SHA256_LEN: usize = 32;
/// tar/zip Unix `st_mode` file-type mask (`S_IFMT`).
const UNIX_FILE_TYPE_MASK: u32 = 0o170000;
/// tar/zip Unix regular-file type (`S_IFREG`).
const UNIX_REGULAR_FILE: u32 = 0o100000;
/// Bound the decompressed tar stream, not only the entries that are kept.
const TAR_DECOMPRESS_LIMIT_FACTOR: u64 = 3;
/// The release metadata file both archive shapes carry.
pub(crate) const PRODUCT_FILE: &str = "product.json";

/// The asset the running target needs.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Wanted {
    /// A plain archive holding the app binary and `product.json` at its root.
    Plain { name: String, zip: bool },
    /// A macOS application-bundle zip.
    Bundle { name: String },
}

impl Wanted {
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Plain { name, .. } | Self::Bundle { name } => name,
        }
    }
}

/// The app binary and `product.json` taken out of a plain archive.
#[derive(Debug)]
pub(crate) struct Contents {
    pub binary: Vec<u8>,
    pub product: Vec<u8>,
}

/// The release target triple for one operating system and architecture. Only
/// the four targets the release workflow builds are supported.
pub(crate) fn rust_target(os: &str, arch: &str) -> Result<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        (os, arch) => bail!("no release asset is published for {os}/{arch}"),
    }
}

pub(crate) fn plain_asset_name(prefix: &str, version: &str, target: &str, zip: bool) -> String {
    let extension = if zip { "zip" } else { "tar.gz" };
    format!("{prefix}-{version}-{target}.{extension}")
}

pub(crate) fn binary_entry_name(binary: &str, zip: bool) -> String {
    if zip {
        format!("{binary}.exe")
    } else {
        binary.to_owned()
    }
}

pub(crate) fn bundle_asset_name(app_bundle: &str, version: &str) -> String {
    format!("{app_bundle}_{version}_darwin_arm64.zip")
}

/// The `<Name>.app` directory when `executable` is exactly
/// `<Name>.app/Contents/MacOS/<file>`, and `None` for every other layout. A
/// path component that merely ends in `.app` is not a bundle.
pub(crate) fn bundle_root(executable: &Path) -> Option<&Path> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    let shaped = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && app.extension()? == "app";
    shaped.then_some(app)
}

/// Which asset to download for this executable, decided before any request so
/// an unsupported target fails without a download.
pub(crate) fn wanted_asset(
    source: &UpdateSource,
    version: &str,
    executable: &Path,
) -> Result<Wanted> {
    let target = rust_target(std::env::consts::OS, std::env::consts::ARCH)?;
    if cfg!(target_os = "macos") && bundle_root(executable).is_some() {
        return Ok(Wanted::Bundle {
            name: bundle_asset_name(&source.app_bundle, version),
        });
    }
    Ok(Wanted::Plain {
        name: plain_asset_name(&source.archive_prefix, version, target, cfg!(windows)),
        zip: cfg!(windows),
    })
}

/// Reads at most `limit` bytes and fails when more are available.
pub(crate) fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    read_limited_with_progress(reader, limit, &|_| {})
}

/// Reports bytes after each bounded read, without allocating from an HTTP size.
pub(crate) fn read_limited_with_progress(
    reader: impl Read,
    limit: u64,
    progress: &dyn Fn(u64),
) -> Result<Vec<u8>> {
    const READ_CHUNK: usize = 64 * 1024;
    let mut bytes = Vec::new();
    let mut reader = reader.take(limit + 1);
    let mut buffer = [0; READ_CHUNK];
    progress(0);
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        if count == 0 {
            break;
        }
        ensure!(
            bytes.len() as u64 + count as u64 <= limit,
            "a download or archive entry exceeds {limit} bytes"
        );
        bytes.extend_from_slice(&buffer[..count]);
        progress(bytes.len() as u64);
    }
    Ok(bytes)
}

/// Checks `data` against the one `<asset>.sha256` line that names `name`.
pub(crate) fn verify_checksum(text: &[u8], name: &str, data: &[u8]) -> Result<()> {
    let text = std::str::from_utf8(text).context("the checksum file is not UTF-8")?;
    let mut expected = None;
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 2 || fields[1].trim_start_matches('*') != name {
            continue;
        }
        ensure!(
            expected.is_none(),
            "the checksum file has more than one entry for {name}"
        );
        let digest = hex::decode(fields[0]).context("the checksum file has an invalid SHA-256")?;
        ensure!(
            digest.len() == SHA256_LEN,
            "the checksum file has a SHA-256 of the wrong length"
        );
        expected = Some(digest);
    }
    let expected =
        expected.with_context(|| format!("the checksum file has no entry for {name}"))?;
    ensure!(
        Sha256::digest(data).as_slice() == expected,
        "checksum mismatch for {name}; nothing was installed"
    );
    Ok(())
}

/// Takes the app binary and `product.json` out of a release archive. Only
/// regular files at the archive root are accepted, so a symbolic link, a
/// hard link, a directory or a nested path with a wanted name is refused.
pub(crate) fn extract(archive: &[u8], zip: bool, binary: &str) -> Result<Contents> {
    extract_limited(archive, zip, binary, MAX_DOWNLOAD)
}

fn extract_limited(archive: &[u8], zip: bool, binary: &str, limit: u64) -> Result<Contents> {
    let found = if zip {
        from_zip(archive, binary, limit)?
    } else {
        from_tar(archive, binary, limit)?
    };
    into_contents(found, binary)
}

fn into_contents(mut found: BTreeMap<String, Vec<u8>>, binary: &str) -> Result<Contents> {
    let binary = found
        .remove(binary)
        .with_context(|| format!("the release archive has no regular {binary} binary"))?;
    let product = found
        .remove(PRODUCT_FILE)
        .with_context(|| format!("the release archive has no {PRODUCT_FILE}"))?;
    Ok(Contents { binary, product })
}

fn is_wanted(name: &str, binary: &str) -> bool {
    name == binary || name == PRODUCT_FILE
}

fn unix_regular_file(mode: Option<u32>) -> bool {
    mode.is_none_or(|mode| {
        mode & UNIX_FILE_TYPE_MASK == 0 || mode & UNIX_FILE_TYPE_MASK == UNIX_REGULAR_FILE
    })
}

fn insert_entry(
    found: &mut BTreeMap<String, Vec<u8>>,
    name: String,
    reader: &mut dyn Read,
    limit: u64,
) -> Result<()> {
    ensure!(
        !found.contains_key(&name),
        "the release archive has a duplicate {name} entry"
    );
    let bytes = read_limited(reader, limit)?;
    ensure!(!bytes.is_empty(), "the release archive has an empty {name}");
    found.insert(name, bytes);
    Ok(())
}

fn from_zip(archive: &[u8], binary: &str, limit: u64) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut found = BTreeMap::new();
    let mut zip = zip::ZipArchive::new(Cursor::new(archive)).context("read the release zip")?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry.name()?.into_owned();
        if !is_wanted(&name, binary) {
            continue;
        }
        ensure!(
            entry.is_file() && unix_regular_file(entry.unix_mode()),
            "the release archive entry {name} is not a regular file"
        );
        insert_entry(&mut found, name, &mut entry, limit)?;
    }
    Ok(found)
}

fn from_tar(archive: &[u8], binary: &str, limit: u64) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut found = BTreeMap::new();
    let decoder = flate2::read::GzDecoder::new(archive);
    let mut tar = tar::Archive::new(decoder.take(limit * TAR_DECOMPRESS_LIMIT_FACTOR));
    for entry in tar.entries().context("read the release archive")? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let Some(name) = path.to_str().map(trimmed_root_name) else {
            continue;
        };
        if !is_wanted(&name, binary) {
            continue;
        }
        ensure!(
            entry.header().entry_type().is_file(),
            "the release archive entry {name} is not a regular file"
        );
        insert_entry(&mut found, name, &mut entry, limit)?;
    }
    Ok(found)
}

fn trimmed_root_name(name: &str) -> String {
    name.trim_start_matches("./").to_owned()
}

#[derive(Deserialize)]
struct Product {
    id: String,
    version: String,
    bundle_id: Option<String>,
}

/// Checks that an archived or bundled `product.json` describes this app and
/// the release that was asked for.
pub(crate) fn verify_product(
    bytes: &[u8],
    id: &str,
    version: &str,
    bundle_id: Option<&str>,
) -> Result<()> {
    let product: Product =
        serde_json::from_slice(bytes).context("decode the release product.json")?;
    ensure!(
        product.id == id,
        "the release product.json names {:?}, not this app",
        product.id
    );
    ensure!(
        product.version == version,
        "the release product.json names version {:?}, not {version}",
        product.version
    );
    if let Some(expected) = bundle_id {
        ensure!(
            product.bundle_id.as_deref() == Some(expected),
            "the release product.json does not name this app's bundle identifier"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};
    use std::path::PathBuf;

    use sha2::{Digest, Sha256};

    use super::{
        PRODUCT_FILE, binary_entry_name, bundle_asset_name, bundle_root, extract, extract_limited,
        plain_asset_name, read_limited, rust_target, verify_checksum, verify_product,
    };
    use crate::test_archive::{archive, product_json};

    const BINARY: &str = "subspace";

    #[test]
    fn every_supported_target_has_a_triple_and_the_rest_fail() {
        let supported = [
            (("linux", "x86_64"), "x86_64-unknown-linux-gnu"),
            (("linux", "aarch64"), "aarch64-unknown-linux-gnu"),
            (("macos", "aarch64"), "aarch64-apple-darwin"),
            (("windows", "x86_64"), "x86_64-pc-windows-msvc"),
        ];
        for ((os, arch), target) in supported {
            assert_eq!(rust_target(os, arch).unwrap(), target);
        }
        for (os, arch) in [
            ("macos", "x86_64"),
            ("windows", "aarch64"),
            ("freebsd", "x86_64"),
            ("linux", "riscv64"),
        ] {
            let error = rust_target(os, arch).unwrap_err();
            assert!(error.to_string().contains(os), "{error}");
        }
    }

    #[test]
    fn asset_names_follow_the_release_workflow() {
        assert_eq!(binary_entry_name("subspace", true), "subspace.exe");
        assert_eq!(binary_entry_name("subspace", false), "subspace");
        assert_eq!(binary_entry_name("subspace", true), "subspace.exe");
        assert_eq!(binary_entry_name("subspace", false), "subspace");
        assert_eq!(
            plain_asset_name("subspace", "1.2.0", "aarch64-apple-darwin", false),
            "subspace-1.2.0-aarch64-apple-darwin.tar.gz"
        );
        assert_eq!(
            plain_asset_name("subspace", "1.2.0", "x86_64-pc-windows-msvc", true),
            "subspace-1.2.0-x86_64-pc-windows-msvc.zip"
        );
        assert_eq!(
            bundle_asset_name("Subspace", "1.2.0"),
            "Subspace_1.2.0_darwin_arm64.zip"
        );
    }

    #[test]
    fn a_bundle_is_detected_only_by_its_exact_layout() {
        let bundle = PathBuf::from("/Applications/Subspace.app");
        assert_eq!(
            bundle_root(&bundle.join("Contents/MacOS").join(BINARY)),
            Some(bundle.as_path())
        );
        for other in [
            "/usr/local/bin/subspace",
            "/Applications/Subspace.app/Contents/Resources/subspace",
            "/Applications/Subspace.app/Contents/MacOS/nested/subspace",
            "/Users/fixture/my.app/bin/subspace",
            "/Users/fixture/plain/Contents/MacOS/subspace",
            "subspace",
        ] {
            assert_eq!(bundle_root(&PathBuf::from(other)), None, "{other}");
        }
    }

    #[test]
    fn bounded_reads_refuse_one_byte_over_the_limit() {
        assert_eq!(read_limited(&b"123"[..], 3).unwrap(), b"123");
        let error = read_limited(&b"1234"[..], 3).unwrap_err();
        assert!(error.to_string().contains("exceeds 3 bytes"), "{error}");
    }

    #[test]
    fn a_checksum_entry_must_exist_once_be_well_formed_and_match() {
        let body = b"archive";
        let digest = hex::encode(Sha256::digest(body));
        for text in [
            format!("{}  app.tar.gz\r\n", digest.to_uppercase()),
            format!("{digest} *app.tar.gz"),
            format!("{digest}  app.tar.gz\n{digest}  other.tar.gz\n"),
        ] {
            assert!(
                verify_checksum(text.as_bytes(), "app.tar.gz", body).is_ok(),
                "{text}"
            );
        }
        for text in [
            String::new(),
            "nope  app.tar.gz".into(),
            "12  app.tar.gz".into(),
            format!("{digest}  other.tar.gz"),
            format!("{digest}  app.tar.gz\n{digest}  app.tar.gz"),
        ] {
            assert!(
                verify_checksum(text.as_bytes(), "app.tar.gz", body).is_err(),
                "{text}"
            );
        }
        let error = verify_checksum(
            format!("{digest}  app.tar.gz").as_bytes(),
            "app.tar.gz",
            b"corrupt",
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("nothing was installed"),
            "{error}"
        );
    }

    #[test]
    fn both_archive_shapes_yield_the_binary_and_the_product_file() {
        for zip in [false, true] {
            let binary = if zip {
                format!("{BINARY}.exe")
            } else {
                BINARY.to_owned()
            };
            let contents = extract(&archive(zip, "1.2.0"), zip, &binary).unwrap();
            assert_eq!(contents.binary, format!("new {BINARY}").as_bytes());
            assert!(verify_product(&contents.product, "subspace", "1.2.0", None).is_ok());
        }
    }

    #[test]
    fn a_missing_binary_or_product_file_is_named() {
        for zip in [false, true] {
            let only_product = raw_archive(zip, &[(PRODUCT_FILE, &product_json("1.2.0"))]);
            let error = extract(&only_product, zip, BINARY).unwrap_err();
            assert!(error.to_string().contains(BINARY), "{error}");

            let only_binary = raw_archive(zip, &[(BINARY, b"new")]);
            let error = extract(&only_binary, zip, BINARY).unwrap_err();
            assert!(error.to_string().contains(PRODUCT_FILE), "{error}");
        }
    }

    #[test]
    fn a_nested_or_traversing_entry_never_counts_as_the_binary() {
        for name in ["dir/subspace", "../subspace"] {
            let bytes = raw_archive(
                true,
                &[(name, b"binary"), (PRODUCT_FILE, &product_json("1.2.0"))],
            );
            assert!(extract(&bytes, true, BINARY).is_err(), "{name}");
        }
        let bytes = raw_archive(
            false,
            &[
                ("dir/subspace", b"binary"),
                (PRODUCT_FILE, &product_json("1.2.0")),
            ],
        );
        assert!(extract(&bytes, false, BINARY).is_err());
    }

    /// Only tar can carry a duplicate name here: the zip writer refuses to
    /// produce one, so a zip with two entries of the same name cannot be
    /// built without writing the container by hand.
    #[test]
    fn a_duplicate_entry_is_refused_rather_than_overwritten() {
        let bytes = raw_archive(
            false,
            &[
                (BINARY, b"first"),
                (BINARY, b"second"),
                (PRODUCT_FILE, &product_json("1.2.0")),
            ],
        );
        let error = extract(&bytes, false, BINARY).unwrap_err();
        assert!(error.to_string().contains("duplicate"), "{error}");
    }

    #[test]
    fn a_symbolic_link_entry_is_refused_in_both_shapes() {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .add_symlink(
                BINARY,
                "elsewhere",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let error = extract(&bytes, true, BINARY).unwrap_err();
        assert!(error.to_string().contains("not a regular file"), "{error}");

        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut tar = tar::Builder::new(gzip);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_link_name("elsewhere").unwrap();
        header.set_cksum();
        tar.append_data(&mut header, BINARY, &b""[..]).unwrap();
        let bytes = tar.into_inner().unwrap().finish().unwrap();
        let error = extract(&bytes, false, BINARY).unwrap_err();
        assert!(error.to_string().contains("not a regular file"), "{error}");
    }

    #[test]
    fn an_entry_over_the_size_limit_is_refused() {
        let oversized = vec![b'x'; 1024];
        for zip in [false, true] {
            let bytes = raw_archive(
                zip,
                &[(BINARY, &oversized), (PRODUCT_FILE, &product_json("1.2.0"))],
            );
            assert!(extract(&bytes, zip, BINARY).is_ok());
            let error = extract_limited(&bytes, zip, BINARY, 512).unwrap_err();
            assert!(error.to_string().contains("exceeds 512 bytes"), "{error}");
        }
    }

    #[test]
    fn product_json_must_match_the_id_version_and_bundle_identifier() {
        let bytes = product_json("1.2.0");
        assert!(verify_product(&bytes, "subspace", "1.2.0", None).is_ok());
        assert!(verify_product(&bytes, "subspace", "1.2.0", Some("com.example.subspace")).is_ok());
        for (id, version, bundle_id, expected) in [
            ("other-app", "1.2.0", None, "not this app"),
            ("subspace", "1.3.0", None, "not 1.3.0"),
            (
                "subspace",
                "1.2.0",
                Some("com.example.other"),
                "bundle identifier",
            ),
        ] {
            let error = verify_product(&bytes, id, version, bundle_id).unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
        }
        assert!(verify_product(b"not json", "subspace", "1.2.0", None).is_err());
    }

    /// An archive with exactly the named entries, including shapes `archive`
    /// refuses to build.
    fn raw_archive(zip: bool, entries: &[(&str, &[u8])]) -> Vec<u8> {
        if zip {
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            for (name, bytes) in entries {
                writer
                    .start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(bytes).unwrap();
            }
            return writer.finish().unwrap().into_inner();
        }
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut tar = tar::Builder::new(gzip);
        for (name, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, *name, *bytes).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }
}
