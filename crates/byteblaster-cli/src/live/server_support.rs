use crate::product_meta::{ProductMeta, detect_product_meta};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub(crate) struct RetainedFile {
    pub(crate) filename: String,
    pub(crate) data: Vec<u8>,
    pub(crate) timestamp_utc: u64,
    pub(crate) completed_at: SystemTime,
}

impl RetainedFile {
    fn size(&self) -> usize {
        self.data.len()
    }
}

#[derive(Debug)]
pub(crate) struct RetainedFiles {
    by_name: HashMap<String, RetainedFile>,
    order: VecDeque<String>,
    max_entries: usize,
    ttl: Duration,
}

impl RetainedFiles {
    pub(crate) fn new(max_entries: usize, ttl: Duration) -> Self {
        Self {
            by_name: HashMap::new(),
            order: VecDeque::new(),
            max_entries: max_entries.max(1),
            ttl: ttl.max(Duration::from_secs(1)),
        }
    }

    pub(crate) fn insert(
        &mut self,
        filename: String,
        data: Vec<u8>,
        timestamp_utc: u64,
        completed_at: SystemTime,
    ) {
        self.evict_expired();

        if self.by_name.contains_key(&filename) {
            self.order.retain(|name| name != &filename);
        }
        self.order.push_back(filename.clone());
        self.by_name.insert(
            filename.clone(),
            RetainedFile {
                filename,
                data,
                timestamp_utc,
                completed_at,
            },
        );

        while self.by_name.len() > self.max_entries {
            if let Some(oldest) = self.order.pop_front() {
                self.by_name.remove(&oldest);
            } else {
                break;
            }
        }
    }

    pub(crate) fn list(&mut self) -> Vec<RetainedFileMeta> {
        self.evict_expired();
        self.order
            .iter()
            .rev()
            .filter_map(|name| self.by_name.get(name))
            .map(|file| RetainedFileMeta {
                filename: file.filename.clone(),
                size: file.size(),
                timestamp_utc: file.timestamp_utc,
                product: detect_product_meta(&file.filename),
            })
            .collect()
    }

    pub(crate) fn get(&mut self, filename: &str) -> Option<RetainedFile> {
        self.evict_expired();
        self.by_name.get(filename).cloned()
    }

    pub(crate) fn len(&mut self) -> usize {
        self.evict_expired();
        self.by_name.len()
    }

    fn evict_expired(&mut self) {
        let now = SystemTime::now();
        self.order.retain(|name| {
            let Some(file) = self.by_name.get(name) else {
                return false;
            };
            let age = now
                .duration_since(file.completed_at)
                .unwrap_or(Duration::from_secs(0));
            if age > self.ttl {
                self.by_name.remove(name);
                return false;
            }
            true
        });
    }
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct RetainedFileMeta {
    pub(crate) filename: String,
    pub(crate) size: usize,
    pub(crate) timestamp_utc: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) product: Option<ProductMeta>,
}

pub(crate) fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p = pattern.to_ascii_lowercase();
    let t = text.to_ascii_lowercase();

    let p_bytes = p.as_bytes();
    let t_bytes = t.as_bytes();
    let mut pi = 0usize;
    let mut ti = 0usize;
    let mut star_idx = None;
    let mut match_idx = 0usize;

    while ti < t_bytes.len() {
        if pi < p_bytes.len() && (p_bytes[pi] == t_bytes[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p_bytes.len() && p_bytes[pi] == b'*' {
            star_idx = Some(pi);
            match_idx = ti;
            pi += 1;
        } else if let Some(star_pos) = star_idx {
            pi = star_pos + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }

    while pi < p_bytes.len() && p_bytes[pi] == b'*' {
        pi += 1;
    }

    pi == p_bytes.len()
}

fn content_type_for_filename(filename: &str) -> &'static str {
    let upper = filename.to_ascii_uppercase();
    if upper.ends_with(".TXT") || upper.ends_with(".WMO") || upper.ends_with(".XML") {
        "text/plain; charset=utf-8"
    } else if upper.ends_with(".JSON") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

pub(crate) fn sanitize_requested_filename(raw: &str) -> Option<String> {
    let trimmed = raw.trim_start_matches('/').trim();
    if trimmed.is_empty() || trimmed.contains('\0') || trimmed.contains("..") {
        return None;
    }
    if trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return None;
    }
    Some(trimmed.to_string())
}

pub(crate) fn file_download_url(filename: &str) -> String {
    format!("/files/{}", percent_encode(filename))
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push_str(&format!("{b:02X}"));
        }
    }
    out
}

pub(crate) fn build_file_download_response(file: RetainedFile) -> Response {
    let content_type = content_type_for_filename(&file.filename);
    let disposition = format!("attachment; filename=\"{}\"", file.filename);

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(CONTENT_DISPOSITION, value);
    }
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));

    (headers, file.data).into_response()
}

pub(crate) fn filename_request_or_400(raw: &str) -> Result<String, StatusCode> {
    sanitize_requested_filename(raw).ok_or(StatusCode::BAD_REQUEST)
}
