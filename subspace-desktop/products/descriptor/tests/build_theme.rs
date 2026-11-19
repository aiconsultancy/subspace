use std::process::Command;

#[test]
fn packaging_metadata_uses_the_compiled_default_not_the_runtime_environment() {
    let expected =
        option_env!("SUBSPACE_DEFAULT_THEME").unwrap_or(subspace_framework::product::DEFAULT_THEME);
    let output = Command::new(env!("CARGO_BIN_EXE_subspace-product-metadata"))
        .args(["subspace", "1.2.3"])
        .env("SUBSPACE_DEFAULT_THEME", "Solarized Dark")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(metadata["default_theme"], expected);
}
