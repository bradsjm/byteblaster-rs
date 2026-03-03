#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Text,
    Json,
}

pub fn emit_text_line(line: &str) {
    println!("{line}");
}

pub fn emit_json_line(value: &serde_json::Value) -> anyhow::Result<()> {
    let serialized = serde_json::to_string(value)?;
    println!("{serialized}");
    Ok(())
}
