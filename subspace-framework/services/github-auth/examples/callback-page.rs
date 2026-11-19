//! Render the real callback page with fictional data, without signing in.
//! `cargo run -p subspace-github-auth --example callback-page -- success > /tmp/success.html`

use subspace_core::Secret;
use subspace_github_auth::AuthError;
use subspace_github_auth::browser::{Callback, page};

fn fixture(state: Option<&str>) -> Callback {
    match state {
        Some("error") => Callback::Failed(AuthError::StateMismatch),
        Some("not-found") => Callback::Other,
        _ => Callback::Code(Secret::new("fixture-code")),
    }
}

fn main() {
    let callback = fixture(std::env::args().nth(1).as_deref());
    print!("{}", page(&callback).1);
}
