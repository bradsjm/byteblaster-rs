//! Internal parsing pipeline for product enrichment.
//!
//! Phase 1 introduces explicit stages without changing the public API. The
//! pipeline keeps the existing behavior but moves orchestration out of
//! `product.rs` so later phases can refactor classification and parsing
//! independently.

mod assemble;
mod classify;
mod envelope;
mod normalize;

pub(crate) use assemble::assemble_product_enrichment;
pub(crate) use classify::classify;
pub(crate) use envelope::{EnvelopeKind, ParsedEnvelope};
pub(crate) use normalize::NormalizedInput;
