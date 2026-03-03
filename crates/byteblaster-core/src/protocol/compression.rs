use crate::error::ProtocolError;
use std::io::Read;

pub fn has_zlib_header(input: &[u8]) -> bool {
    matches!(
        input.get(0..2),
        Some([0x78, 0x9C] | [0x78, 0xDA] | [0x78, 0x01])
    )
}

pub fn decompress_zlib(input: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    let mut decoder = flate2::read::ZlibDecoder::new(input);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| ProtocolError::Decompression(e.to_string()))?;
    Ok(out)
}
