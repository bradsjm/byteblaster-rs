pub mod model;
pub mod qbt_adapter;
pub mod wxwire_adapter;

pub use model::{
    IngestError, IngestEvent, IngestTelemetry, IngestWarning, ProductOrigin, ReceivedProduct,
};
pub use qbt_adapter::{QbtIngestStream, adapt_qbt_events};
pub use wxwire_adapter::{WxWireIngestStream, adapt_wxwire_events};
