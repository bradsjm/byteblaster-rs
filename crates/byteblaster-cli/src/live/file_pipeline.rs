use crate::product_meta::detect_product_meta;
use std::path::Path;
use std::time::SystemTime;

pub(crate) struct CompletedFileRecord {
    pub(crate) filename: String,
    pub(crate) path: String,
    pub(crate) timestamp_utc: u64,
    pub(crate) event: serde_json::Value,
}

pub(crate) fn write_completed_file(
    output_dir: &Path,
    filename: &str,
    data: &[u8],
) -> anyhow::Result<String> {
    let target = output_dir.join(filename);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, data)?;
    Ok(target.to_string_lossy().to_string())
}

pub(crate) fn completed_file_event(
    filename: &str,
    path: &str,
    timestamp_utc: u64,
) -> serde_json::Value {
    let mut file_event = serde_json::json!({
        "filename": filename,
        "path": path,
        "timestamp_utc": timestamp_utc,
    });
    if let Some(product) = detect_product_meta(filename)
        && let Ok(product_json) = serde_json::to_value(product)
    {
        file_event["product"] = product_json;
    }
    file_event
}

pub(crate) fn persist_completed_file(
    output_dir: &Path,
    filename: &str,
    data: &[u8],
    timestamp: SystemTime,
) -> anyhow::Result<CompletedFileRecord> {
    let path = write_completed_file(output_dir, filename, data)?;
    let timestamp_utc = crate::live::shared::unix_seconds(timestamp);
    Ok(CompletedFileRecord {
        filename: filename.to_string(),
        event: completed_file_event(filename, &path, timestamp_utc),
        path,
        timestamp_utc,
    })
}
