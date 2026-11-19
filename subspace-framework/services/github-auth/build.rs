//! The OAuth App's client secret is compiled in from the build environment,
//! never from the repository (see `docs/installation.md`).

fn main() {
    println!("cargo:rerun-if-env-changed=SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET");
}
