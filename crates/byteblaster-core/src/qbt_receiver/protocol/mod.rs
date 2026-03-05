//! Protocol layer for ByteBlaster.
//!
//! This module provides protocol parsing, encoding, and validation
//! for the ByteBlaster wire format, including XOR transformation, frame
//! decoding, checksum validation, and data model definitions.
//!
//! ## Wire Format Overview
//!
//! The ByteBlaster protocol uses an XOR-obfuscated TCP stream with
//! the following characteristics:
//! - All bytes are XOR'd with `0xFF` on the wire
//! - Frames are delimited by `/PF` and `/ServerList` headers
//! - Supports both V1 (legacy) and V2 (compressed) frame formats
//! - Text payloads are normalized (CRLF to LF)
//!
//! ## Submodules
//!
//! - [`auth`]: Authentication and logon message building, including XOR transformation
//! - [`checksum`]: Checksum calculation and verification for data integrity
//! - [`codec`]: Stateful frame decoder with sync recovery and resync handling
//! - [`compression`]: V2 zlib decompression with configurable policies
//! - [`model`]: Protocol data models (QbtFrameEvent, QbtSegment, QbtServerList, etc.)
//! - [`server_list`]: Server list frame parsing with multiple format support
//!
//! ## Frame Types
//!
//! - `/PF`: Product data frames containing file segments (QbtSegment)
//! - `/ServerList`: Server list update frames with primary and satellite endpoints
//!
//! ## Protocol Versions
//!
//! - **V1**: Legacy uncompressed format with basic headers
//! - **V2**: Compressed format using zlib for efficient transmission
//!
//! ## Checksum Validation
//!
//! Frames include checksums for data integrity. The decoder supports
//! multiple policies via [`QbtChecksumPolicy`]:
//! - `StrictDrop`: Drops segments with invalid checksums and emits a warning
//!
//! ## Compression Policies
//!
//! V2 frames may be compressed using zlib. The decoder supports
//! multiple policies via [`QbtV2CompressionPolicy`]:
//! - `RequireZlibHeader`: Only attempts decompression if zlib header is present
//! - `TryAlways`: Always attempts decompression regardless of header
//!
//! ## Decoder State Machine
//!
//! The [`QbtProtocolDecoder`] maintains internal state to handle:
//! - Chunk-boundary prefix splits (frame headers split across TCP packets)
//! - Sync recovery after corruption or unknown frames
//! - Frame type identification and validation
//!
//! For authoritative protocol specification and requirements, see
//! `../../docs/protocol.md`.

pub mod auth;
pub mod checksum;
pub mod codec;
pub mod compression;
pub mod model;
pub mod server_list;
pub mod server_list_wire;
