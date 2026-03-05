use crate::qbt_receiver::{
    QbtCompletedFile, QbtProtocolWarning, QbtReceiverError, QbtReceiverTelemetrySnapshot,
};
use crate::wxwire_receiver::{
    WxWireReceiverError, WxWireReceiverFile, WxWireReceiverTelemetrySnapshot, WxWireReceiverWarning,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ReceivedProduct {
    pub filename: String,
    pub data: Bytes,
    pub source_timestamp_utc: SystemTime,
    pub origin: ProductOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ProductOrigin {
    Qbt,
    WxWire {
        message_id: String,
        subject: String,
        delay_stamp_utc: Option<SystemTime>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IngestEvent {
    Product(ReceivedProduct),
    Connected { endpoint: String },
    Disconnected,
    Telemetry(IngestTelemetry),
    Warning(IngestWarning),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IngestTelemetry {
    Qbt(QbtReceiverTelemetrySnapshot),
    WxWire(WxWireReceiverTelemetrySnapshot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IngestWarning {
    Qbt(QbtProtocolWarning),
    WxWire(WxWireReceiverWarning),
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum IngestError {
    #[error("qbt ingest error: {0}")]
    Qbt(#[from] QbtReceiverError),
    #[error("wxwire ingest error: {0}")]
    WxWire(#[from] WxWireReceiverError),
}

impl From<QbtCompletedFile> for ReceivedProduct {
    fn from(value: QbtCompletedFile) -> Self {
        Self {
            filename: value.filename,
            data: value.data,
            source_timestamp_utc: value.timestamp_utc,
            origin: ProductOrigin::Qbt,
        }
    }
}

impl From<WxWireReceiverFile> for ReceivedProduct {
    fn from(value: WxWireReceiverFile) -> Self {
        Self {
            filename: value.filename,
            data: value.data,
            source_timestamp_utc: value.issue_utc,
            origin: ProductOrigin::WxWire {
                message_id: value.id,
                subject: value.subject,
                delay_stamp_utc: value.delay_stamp_utc,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ProductOrigin, ReceivedProduct};
    use crate::qbt_receiver::QbtCompletedFile;
    use crate::wxwire_receiver::WxWireReceiverFile;
    use bytes::Bytes;
    use std::time::{Duration, SystemTime};

    #[test]
    fn qbt_completed_file_converts_to_received_product() {
        let timestamp = SystemTime::UNIX_EPOCH + Duration::from_secs(123);
        let product: ReceivedProduct = QbtCompletedFile {
            filename: "A.TXT".to_string(),
            data: Bytes::from_static(b"abc"),
            timestamp_utc: timestamp,
        }
        .into();

        assert_eq!(product.filename, "A.TXT");
        assert_eq!(product.data, Bytes::from_static(b"abc"));
        assert_eq!(product.source_timestamp_utc, timestamp);
        assert!(matches!(product.origin, ProductOrigin::Qbt));
    }

    #[test]
    fn wxwire_file_converts_to_received_product() {
        let issue = SystemTime::UNIX_EPOCH + Duration::from_secs(111);
        let delay = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(222));
        let product: ReceivedProduct = WxWireReceiverFile {
            filename: "B.TXT".to_string(),
            data: Bytes::from_static(b"xyz"),
            subject: "subject".to_string(),
            id: "id-1".to_string(),
            issue_utc: issue,
            ttaaii: "TTAAII".to_string(),
            cccc: "KAAA".to_string(),
            awipsid: "AFDXXX".to_string(),
            delay_stamp_utc: delay,
        }
        .into();

        assert_eq!(product.filename, "B.TXT");
        assert_eq!(product.data, Bytes::from_static(b"xyz"));
        assert_eq!(product.source_timestamp_utc, issue);
        assert!(matches!(
            product.origin,
            ProductOrigin::WxWire {
                message_id,
                subject,
                delay_stamp_utc,
            } if message_id == "id-1" && subject == "subject" && delay_stamp_utc == delay
        ));
    }
}
