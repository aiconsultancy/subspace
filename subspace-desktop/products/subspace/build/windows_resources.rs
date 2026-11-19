//! Select and embed the Windows icon resources.

pub(crate) fn embed() -> Result<(), embed_resource::CompilationResult> {
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app-nightly.rc");
    println!("cargo:rerun-if-env-changed=SUBSPACE_RELEASE_CHANNEL");

    if std::env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some("windows".as_ref()) {
        let resource = if std::env::var("SUBSPACE_RELEASE_CHANNEL").as_deref() == Ok("nightly") {
            "app-nightly.rc"
        } else {
            "app.rc"
        };
        embed_resource::compile(resource, embed_resource::NONE).manifest_required()?;
    }
    Ok(())
}
