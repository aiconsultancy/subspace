//! Release archives and `product.json` bodies for this crate's tests.

use std::io::{Cursor, Write};

const BINARY: &str = "subspace";
const PRODUCT_ID: &str = "subspace";
const BUNDLE_ID: &str = "com.example.subspace";

/// The `product.json` a release archive and a release bundle carry.
pub(crate) fn product_json(version: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "id": PRODUCT_ID,
        "version": version,
        "bundle_id": BUNDLE_ID,
    }))
    .expect("the fixture product metadata serializes")
}

/// A well-formed plain release archive for the standalone product.
pub(crate) fn archive(zip: bool, version: &str) -> Vec<u8> {
    let binary = format!("new {BINARY}").into_bytes();
    let product = product_json(version);
    let name = if zip {
        format!("{BINARY}.exe")
    } else {
        BINARY.to_owned()
    };
    let entries: [(&str, &[u8]); 2] = [(&name, &binary), ("product.json", &product)];
    if zip {
        return zip_archive(&entries);
    }
    tar_archive(&entries)
}

fn zip_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .expect("start a zip entry");
        writer.write_all(bytes).expect("write a zip entry");
    }
    writer
        .finish()
        .expect("finish the fixture zip")
        .into_inner()
}

fn tar_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(gzip);
    for (name, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, *name, *bytes)
            .expect("append a tar entry");
    }
    tar.into_inner()
        .expect("finish the fixture tar")
        .finish()
        .expect("finish the fixture gzip")
}
