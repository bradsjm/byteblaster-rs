use byteblaster_core::ingest::{IngestEvent, ProductOrigin, adapt_qbt_events, adapt_wxwire_events};
use byteblaster_core::qbt_receiver::{
    QbtFrameEvent, QbtProtocolVersion, QbtReceiverEvent, QbtSegment,
};
use byteblaster_core::wxwire_receiver::{
    WxWireReceiverEvent, WxWireReceiverFile, WxWireReceiverFrameEvent,
};
use bytes::Bytes;
use futures::{StreamExt, stream};
use serde_json::json;

fn qbt_segment(block_number: u32, total_blocks: u32, content: &'static [u8]) -> QbtSegment {
    serde_json::from_value(json!({
        "filename": "QBT.TXT",
        "block_number": block_number,
        "total_blocks": total_blocks,
        "content": content,
        "checksum": 0,
        "length": content.len(),
        "version": QbtProtocolVersion::V2,
        "timestamp_utc": {"secs_since_epoch": 100, "nanos_since_epoch": 0},
        "source": null
    }))
    .expect("valid qbt segment")
}

fn wxwire_file(filename: &str, id: &str, subject: &str, data: &'static [u8]) -> WxWireReceiverFile {
    serde_json::from_value(json!({
        "filename": filename,
        "data": data,
        "subject": subject,
        "id": id,
        "issue_utc": {"secs_since_epoch": 200, "nanos_since_epoch": 0},
        "ttaaii": "TTAAII",
        "cccc": "KAAA",
        "awipsid": "AFDXXX",
        "delay_stamp_utc": null
    }))
    .expect("valid wxwire file")
}

#[tokio::test]
async fn qbt_adapter_emits_completed_product_from_segments() {
    let events = stream::iter(vec![
        Ok(QbtReceiverEvent::Frame(QbtFrameEvent::DataBlock(
            qbt_segment(1, 2, b"abc"),
        ))),
        Ok(QbtReceiverEvent::Frame(QbtFrameEvent::DataBlock(
            qbt_segment(2, 2, b"def"),
        ))),
    ]);

    let output: Vec<_> = adapt_qbt_events(events).collect().await;

    assert_eq!(output.len(), 1);
    let Ok(IngestEvent::Product(product)) = &output[0] else {
        panic!("expected product event");
    };
    assert_eq!(product.filename, "QBT.TXT");
    assert_eq!(product.data, Bytes::from_static(b"abcdef"));
    assert!(matches!(product.origin, ProductOrigin::Qbt));
}

#[tokio::test]
async fn wxwire_adapter_emits_products_directly() {
    let events = stream::iter(vec![Ok(WxWireReceiverEvent::Frame(
        WxWireReceiverFrameEvent::File(wxwire_file("WX.TXT", "id-1", "subject", b"wx")),
    ))]);

    let output: Vec<_> = adapt_wxwire_events(events).collect().await;

    assert_eq!(output.len(), 1);
    let Ok(IngestEvent::Product(product)) = &output[0] else {
        panic!("expected product event");
    };
    assert_eq!(product.filename, "WX.TXT");
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
async fn merged_adapters_produce_uniform_product_events() {
    let qbt_events = stream::iter(vec![Ok(QbtReceiverEvent::Frame(QbtFrameEvent::DataBlock(
        qbt_segment(1, 1, b"q"),
    )))]);
    let wx_events = stream::iter(vec![Ok(WxWireReceiverEvent::Frame(
        WxWireReceiverFrameEvent::File(wxwire_file("WX2.TXT", "id-2", "subject2", b"w")),
    ))]);

    let merged =
        futures::stream::select(adapt_qbt_events(qbt_events), adapt_wxwire_events(wx_events));
    let output: Vec<_> = merged.collect().await;

    let mut filenames = output
        .into_iter()
        .filter_map(|item| match item {
            Ok(IngestEvent::Product(product)) => Some(product.filename),
            _ => None,
        })
        .collect::<Vec<_>>();
    filenames.sort();

    assert_eq!(
        filenames,
        vec!["QBT.TXT".to_string(), "WX2.TXT".to_string()]
    );
}
