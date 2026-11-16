//! A baseline JPEG decoder (work in progress, flyingrobots/lumen#197).

/// The identity of the decoding rules: the integer inverse DCT, the chroma upsampling rule and
/// the integer colour conversion. Changing any of them makes a new identity.
pub const JPEG_DECODE_ID: &str = "lumen-jpeg-decode/1";
