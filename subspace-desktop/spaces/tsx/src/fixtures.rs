//! Signed packages for tests: a throwaway root and release key, and
//! packages built and signed with them in memory.

use nsh_package::{App, Certificate, Contents, SecretKey, build, certify};
use serde_json::json;

/// A root, a release key it certifies for `dev.example.*`, and the root's
/// public key as Settings holds it.
pub(crate) struct Signer {
    pub(crate) root: String,
    release: SecretKey,
    certificate: Certificate,
}

pub(crate) fn signer() -> Signer {
    let root = SecretKey::generate().unwrap();
    let release = SecretKey::generate().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let certificate = certify(
        &root,
        &release.public(),
        &["dev.example.*".to_owned()],
        now - 60,
        now + 3600,
    );
    Signer {
        root: root.public().to_base64(),
        release,
        certificate,
    }
}

/// Build `build` of app `id`, declaring the metrics tier, with bytecode
/// beside its source when `bytecode` is set.
pub(crate) fn package(signer: &Signer, id: &str, build_number: u64, bytecode: bool) -> Vec<u8> {
    let contents = Contents {
        app: App {
            id: id.into(),
            name: "Fixture".into(),
            version: format!("1.0.{build_number}"),
            build: build_number,
        },
        capabilities: Some(json!({ "host": ["subspace.system.metrics"] })),
        source: "globalThis.fixture = 1;".into(),
        bytecode: bytecode.then(|| {
            (
                b"crafted bytecode".to_vec(),
                nsh_engine::BYTECODE_VERSION.to_owned(),
            )
        }),
        source_map: None,
        assets: Vec::new(),
        min_runtime: None,
    };
    build(&contents, &signer.release, &signer.certificate).unwrap()
}
