//! Handle the zlib-compressed payloads used by V2 protocol frames.

use crate::qbt_receiver::error::QbtProtocolError;
use std::io::Read;

/// Returns `true` when the payload starts with one of the zlib headers seen on the feed.
///
/// The decoder keeps this check separate from decompression so policy code can decide whether a
/// missing header is fatal or just suspicious.
pub fn has_zlib_header(input: &[u8]) -> bool {
    matches!(
        input.get(0..2),
        Some([0x78, 0x9C] | [0x78, 0xDA] | [0x78, 0x01])
    )
}

/// Decompresses a zlib payload into an owned buffer.
///
/// # Errors
///
/// Returns [`QbtProtocolError::Decompression`] when the body is not valid zlib data.
pub fn decompress_zlib(input: &[u8]) -> Result<Vec<u8>, QbtProtocolError> {
    let mut decoder = flate2::read::ZlibDecoder::new(input);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| QbtProtocolError::Decompression(e.to_string()))?;
    Ok(out)
}
