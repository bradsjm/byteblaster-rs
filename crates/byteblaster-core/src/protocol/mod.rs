//! Protocol layer for ByteBlaster.
//!
//! This module provides protocol parsing, encoding, and validation
//! for the ByteBlaster wire format, including:
//! - Frame decoding and encoding
//! - Checksum calculation and verification
//! - Zlib compression handling
//! - Server list parsing
//! - Protocol data models

pub mod auth;
pub mod checksum;
pub mod codec;
pub mod compression;
pub mod model;
pub mod server_list;
