use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductParseIssue {
    pub kind: &'static str,
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
}

impl ProductParseIssue {
    pub(crate) fn new(
        kind: &'static str,
        code: &'static str,
        message: impl Into<String>,
        line: Option<String>,
    ) -> Self {
        Self {
            kind,
            code,
            message: message.into(),
            line,
        }
    }
}
