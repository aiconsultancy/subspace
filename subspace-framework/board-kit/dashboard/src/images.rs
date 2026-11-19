//! Bounded raster decoding.

mod decode;

pub use decode::MAX_ENCODED_BYTES;
pub use decode::{DecodeLimits, decode, decode_with_limits};
