//! Compression utilities for ByteBlaster V2 protocol.
//!
//! This module provides zlib compression detection and decompression
//! for V2 protocol frames that use compression.

use crate::error::ProtocolError;
use std::io::Read;

/// Checks if the input has a valid zlib header.
///
/// Zlib headers start with specific byte pairs that indicate
/// compression level and window size.
///
/// # Arguments
///
/// * `input` - The byte slice to check
///
/// # Returns
///
/// `true` if the input starts with a valid zlib header
///
/// # Valid Headers
///
/// - `0x78 0x9C` - Default compression
/// - `0x78 0xDA` - Best compression
/// - `0x78 0x01` - No compression
pub fn has_zlib_header(input: &[u8]) -> bool {
    matches!(
        input.get(0..2),
        Some([0x78, 0x9C] | [0x78, 0xDA] | [0x78, 0x01])
    )
}

/// Decompresses zlib-compressed data.
///
/// # Arguments
///
/// * `input` - The compressed byte slice
///
/// # Returns
///
/// Decompressed bytes on success, or a `ProtocolError::Decompression` on failure
///
/// # Errors
///
/// Returns an error if the data is not valid zlib compressed data
pub fn decompress_zlib(input: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    let mut decoder = flate2::read::ZlibDecoder::new(input);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| ProtocolError::Decompression(e.to_string()))?;
    Ok(out)
}
