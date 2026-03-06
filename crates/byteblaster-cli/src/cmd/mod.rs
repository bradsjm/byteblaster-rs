//! Command implementations for the ByteBlaster CLI.
//!
//! This module contains the implementation of each CLI subcommand.
//!
//! ## Available Commands
//!
//! ### `download <output_dir>`
//! Downloads and assembles complete files from the ByteBlaster feed.
//! - Connects to live servers and continuously downloads files
//! - Outputs structured JSON summaries to `stdout`
//!
//! ## Output Contract
//!
//! All commands follow a strict output separation:
//! - **`stdout`**: Command payloads (JSON for `download`)
//! - **`stderr`**: Diagnostics, warnings, and structured logs
//!
//! This separation ensures that command output remains machine-readable
//! and parseable, while diagnostic information can be logged separately.
//!
//! ## Integration
//!
//! Shared formatting and download behaviors live here. The `stream` and `server`
//! command runtimes are implemented directly under `crate::live`.

pub mod download;
pub mod event_output;
