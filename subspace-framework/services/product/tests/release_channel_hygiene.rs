//! `release_channel!` expands in an app whose own items shadow the
//! prelude's `Some`, `None`, `panic!`, `concat!` and `option_env!`.

#![allow(unused_macros)]

use subspace_product::{Channel, release_channel};

#[derive(Debug, PartialEq)]
enum Shadow {
    Some(Channel),
    None,
}

use Shadow::{None, Some};

macro_rules! panic {
    ($($any:tt)*) => {
        compile_error!("release_channel! used the caller's panic!")
    };
}

macro_rules! concat {
    ($($any:tt)*) => {
        compile_error!("release_channel! used the caller's concat!")
    };
}

macro_rules! option_env {
    ($($any:tt)*) => {
        compile_error!("release_channel! used the caller's option_env!")
    };
}

#[test]
fn the_macro_names_what_it_uses_by_path() {
    let channel = release_channel!("EXAMPLE_RELEASE_CHANNEL_UNSET_IN_TESTS");
    assert_eq!(channel, Channel::Stable);
    assert_ne!(Some(channel), None);
}
