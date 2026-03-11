//! Async stream adapters layered on top of QBT events.
//!
//! These types provide bounded channels for segments and completed files so callers can integrate
//! the receiver runtime with the wider async `Stream` ecosystem without exposing internal tasks or
//! assembler state.

pub mod file_stream;
pub mod segment_stream;
