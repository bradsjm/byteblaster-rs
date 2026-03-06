use crate::ingest::model::{IngestError, IngestEvent, IngestTelemetry, IngestWarning};
use crate::qbt_receiver::{
    QbtFileAssembler, QbtFrameEvent, QbtReceiver, QbtReceiverClient, QbtReceiverError,
    QbtReceiverEvent, QbtReceiverResult, QbtSegmentAssembler,
};
use crate::runtime_support::ReceiverEventStream;
use futures::{Stream, StreamExt, future};
use std::pin::Pin;

const DEFAULT_DUPLICATE_CACHE_SIZE: usize = 100;

pub struct QbtIngestStream {
    receiver: QbtReceiver,
}

impl QbtIngestStream {
    pub fn new(receiver: QbtReceiver) -> Self {
        Self { receiver }
    }

    pub fn start(&mut self) -> QbtReceiverResult<()> {
        self.receiver.start()
    }

    pub fn stop(
        &mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), IngestError>> + Send + '_>> {
        Box::pin(async move { self.receiver.stop().await.map_err(IngestError::from) })
    }

    pub fn events(&mut self) -> Result<ReceiverEventStream<IngestEvent, IngestError>, IngestError> {
        Ok(Box::pin(adapt_qbt_events(self.receiver.events()?)))
    }
}

pub fn adapt_qbt_events<'a, S>(
    events: S,
) -> impl Stream<Item = Result<IngestEvent, IngestError>> + Send + 'a
where
    S: Stream<Item = Result<QbtReceiverEvent, QbtReceiverError>> + Send + 'a,
{
    events
        .scan(
            QbtFileAssembler::new(DEFAULT_DUPLICATE_CACHE_SIZE),
            |assembler, item| future::ready(Some(map_qbt_event(item, assembler))),
        )
        .filter_map(future::ready)
}

fn map_qbt_event(
    item: Result<QbtReceiverEvent, QbtReceiverError>,
    assembler: &mut QbtFileAssembler,
) -> Option<Result<IngestEvent, IngestError>> {
    match item {
        Ok(QbtReceiverEvent::Connected(endpoint)) => Some(Ok(IngestEvent::Connected { endpoint })),
        Ok(QbtReceiverEvent::Disconnected) => Some(Ok(IngestEvent::Disconnected)),
        Ok(QbtReceiverEvent::Telemetry(snapshot)) => {
            Some(Ok(IngestEvent::Telemetry(IngestTelemetry::Qbt(snapshot))))
        }
        Ok(QbtReceiverEvent::Frame(frame)) => map_qbt_frame_event(frame, assembler),
        Err(err) => Some(Err(err.into())),
    }
}

fn map_qbt_frame_event(
    frame: QbtFrameEvent,
    assembler: &mut QbtFileAssembler,
) -> Option<Result<IngestEvent, IngestError>> {
    match frame {
        QbtFrameEvent::DataBlock(segment) => match assembler.push(segment) {
            Ok(Some(file)) => Some(Ok(IngestEvent::Product(file.into()))),
            Ok(None) => None,
            Err(err) => Some(Err(err.into())),
        },
        QbtFrameEvent::Warning(warning) => {
            Some(Ok(IngestEvent::Warning(IngestWarning::Qbt(warning))))
        }
        QbtFrameEvent::ServerListUpdate(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{adapt_qbt_events, map_qbt_event};
    use crate::ingest::model::{IngestEvent, IngestWarning};
    use crate::qbt_receiver::{
        QbtFileAssembler, QbtFrameEvent, QbtProtocolVersion, QbtProtocolWarning, QbtReceiverEvent,
        QbtSegment,
    };
    use bytes::Bytes;
    use futures::StreamExt;
    use std::time::{Duration, SystemTime};

    fn segment(block_number: u32, total_blocks: u32, content: &'static [u8]) -> QbtSegment {
        QbtSegment {
            filename: "TEST.TXT".to_string(),
            block_number,
            total_blocks,
            content: Bytes::from_static(content),
            checksum: 0,
            length: content.len(),
            version: QbtProtocolVersion::V2,
            timestamp_utc: SystemTime::UNIX_EPOCH + Duration::from_secs(10),
            source: None,
        }
    }

    #[test]
    fn emits_product_only_when_file_assembly_completes() {
        let mut assembler = QbtFileAssembler::new(8);

        assert!(
            map_qbt_event(
                Ok(QbtReceiverEvent::Frame(QbtFrameEvent::DataBlock(segment(
                    1, 2, b"abc"
                )))),
                &mut assembler,
            )
            .is_none()
        );

        let Some(Ok(IngestEvent::Product(product))) = map_qbt_event(
            Ok(QbtReceiverEvent::Frame(QbtFrameEvent::DataBlock(segment(
                2, 2, b"def",
            )))),
            &mut assembler,
        ) else {
            panic!("expected completed product event");
        };

        assert_eq!(product.filename, "TEST.TXT");
        assert_eq!(product.data, Bytes::from_static(b"abcdef"));
    }

    #[test]
    fn emits_warning_events() {
        let mut assembler = QbtFileAssembler::new(4);
        let warning = QbtProtocolWarning::TimestampParseFallback {
            raw: "bad-ts".to_string(),
        };

        let Some(Ok(IngestEvent::Warning(IngestWarning::Qbt(w)))) = map_qbt_event(
            Ok(QbtReceiverEvent::Frame(QbtFrameEvent::Warning(
                warning.clone(),
            ))),
            &mut assembler,
        ) else {
            panic!("expected warning event");
        };

        assert_eq!(w, warning);
    }

    #[tokio::test]
    async fn adapter_filters_server_list_updates() {
        let events = futures::stream::iter(vec![
            Ok(QbtReceiverEvent::Frame(QbtFrameEvent::ServerListUpdate(
                Default::default(),
            ))),
            Ok(QbtReceiverEvent::Disconnected),
        ]);

        let output: Vec<_> = adapt_qbt_events(events).collect().await;
        assert_eq!(output.len(), 1);
        assert!(matches!(output[0], Ok(IngestEvent::Disconnected)));
    }
}
