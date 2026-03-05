mod enrich;
mod lookup;
mod parser;

pub use enrich::{BbbKind, TextProductEnrichment, enrich_header};
pub use lookup::{
    PIL_ENTRY_COUNT, PIL_GENERATED_AT_UTC, PIL_SOURCE_COMMIT, PIL_SOURCE_PATH, PIL_SOURCE_REPO,
};
pub use parser::{ParserError, TextProductHeader, parse_text_product};
