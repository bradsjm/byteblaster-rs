use crate::ingest::model::{IngestError, IngestEvent, IngestTelemetry, IngestWarning};
use crate::wxwire_receiver::{WxWireReceiverError, WxWireReceiverEvent, WxWireReceiverFrameEvent};
use futures::{Stream, StreamExt, future};

pub(crate) fn adapt_wxwire_events<'a, S>(
    events: S,
) -> impl Stream<Item = Result<IngestEvent, IngestError>> + Send + 'a
where
    S: Stream<Item = Result<WxWireReceiverEvent, WxWireReceiverError>> + Send + 'a,
{
    events.filter_map(|item| future::ready(map_wxwire_event(item)))
}

fn map_wxwire_event(
    item: Result<WxWireReceiverEvent, WxWireReceiverError>,
) -> Option<Result<IngestEvent, IngestError>> {
    match item {
        Ok(WxWireReceiverEvent::Connected(endpoint)) => {
            Some(Ok(IngestEvent::Connected { endpoint }))
        }
        Ok(WxWireReceiverEvent::Disconnected) => Some(Ok(IngestEvent::Disconnected)),
        Ok(WxWireReceiverEvent::Telemetry(snapshot)) => Some(Ok(IngestEvent::Telemetry(
            IngestTelemetry::WxWire(snapshot),
        ))),
        Ok(WxWireReceiverEvent::Frame(frame)) => map_wxwire_frame_event(frame),
        Err(err) => Some(Err(err.into())),
    }
}

fn map_wxwire_frame_event(
    frame: WxWireReceiverFrameEvent,
) -> Option<Result<IngestEvent, IngestError>> {
    match frame {
        WxWireReceiverFrameEvent::File(file) => Some(Ok(IngestEvent::Product(file.into()))),
        WxWireReceiverFrameEvent::Warning(warning) => {
            Some(Ok(IngestEvent::Warning(IngestWarning::WxWire(warning))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::adapt_wxwire_events;
    use crate::ingest::model::{IngestEvent, IngestWarning, ProductOrigin};
    use crate::wxwire_receiver::{
        WxWireReceiverEvent, WxWireReceiverFile, WxWireReceiverFrameEvent, WxWireReceiverWarning,
    };
    use bytes::Bytes;
    use futures::StreamExt;
    use std::time::{Duration, SystemTime};

    #[tokio::test]
    async fn adapter_maps_file_events_to_products() {
        let issue = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let events = futures::stream::iter(vec![Ok(WxWireReceiverEvent::Frame(
            WxWireReceiverFrameEvent::File(WxWireReceiverFile {
                filename: "AFDBOX.TXT".to_string(),
                data: Bytes::from_static(b"body"),
                subject: "subject".to_string(),
                id: "id-1".to_string(),
                issue_utc: issue,
                ttaaii: "FXUS61".to_string(),
                cccc: "KBOX".to_string(),
                awipsid: "AFDBOX".to_string(),
                delay_stamp_utc: None,
            }),
        ))]);

        let output: Vec<_> = adapt_wxwire_events(events).collect().await;
        assert_eq!(output.len(), 1);
        let Ok(IngestEvent::Product(product)) = &output[0] else {
            panic!("expected product event");
        };
        assert_eq!(product.filename, "AFDBOX.TXT");
        assert!(matches!(
            product.origin,
            ProductOrigin::WxWire {
                ref message_id,
                ref subject,
                delay_stamp_utc: None,
            } if message_id == "id-1" && subject == "subject"
        ));
    }

    #[tokio::test]
    async fn adapter_maps_warning_events() {
        let events = futures::stream::iter(vec![Ok(WxWireReceiverEvent::Frame(
            WxWireReceiverFrameEvent::Warning(WxWireReceiverWarning::EmptyBody),
        ))]);

        let output: Vec<_> = adapt_wxwire_events(events).collect().await;
        assert_eq!(output.len(), 1);
        assert!(matches!(
            output[0],
            Ok(IngestEvent::Warning(IngestWarning::WxWire(
                WxWireReceiverWarning::EmptyBody
            )))
        ));
    }
}
