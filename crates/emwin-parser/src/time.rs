//! Resolve partial WMO timestamps against a reference instant.
//!
//! WMO headers usually carry `DDHHMM` data without a month or year. These helpers keep that
//! ambiguity local by generating a small set of plausible UTC instants and choosing the one that
//! best fits the caller's reference time.

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Utc};

/// Resolves a `DDHHMM`-style timestamp to the closest plausible UTC instant.
///
/// The helper only looks at the previous, current, and next month so the caller can recover a
/// stable timestamp without carrying extra calendar state through the parser.
pub(crate) fn resolve_day_time_nearest(
    reference: DateTime<Utc>,
    day: u32,
    hour: u32,
    minute: u32,
) -> Option<DateTime<Utc>> {
    candidate_datetimes(reference, day, hour, minute)?
        .into_iter()
        .min_by_key(|candidate| {
            candidate
                .signed_duration_since(reference)
                .num_seconds()
                .abs()
        })
}

/// Resolves an `HHMM` clock reading to the nearest UTC instant around `reference`.
///
/// This variant only considers yesterday, today, and tomorrow because the day is not encoded.
pub(crate) fn resolve_clock_time_nearest(
    reference: DateTime<Utc>,
    hour: u32,
    minute: u32,
) -> Option<DateTime<Utc>> {
    if hour > 23 || minute > 59 {
        return None;
    }

    let base_date = reference.date_naive();
    let time = NaiveTime::from_hms_opt(hour, minute, 0)?;
    let mut candidates = Vec::with_capacity(3);

    for day_offset in [-1_i64, 0, 1] {
        let date = base_date.checked_add_signed(chrono::TimeDelta::days(day_offset))?;
        candidates.push(Utc.from_utc_datetime(&date.and_time(time)));
    }

    candidates.into_iter().min_by_key(|candidate| {
        candidate
            .signed_duration_since(reference)
            .num_seconds()
            .abs()
    })
}

/// Resolves a `DDHHMM`-style timestamp while preferring candidates at or after `reference`.
///
/// This is useful for bulletin flows that treat the reference instant as a lower bound but still
/// need a deterministic fallback when every candidate lands in the past.
pub(crate) fn resolve_day_time_not_before(
    reference: DateTime<Utc>,
    day: u32,
    hour: u32,
    minute: u32,
) -> Option<DateTime<Utc>> {
    let candidates = candidate_datetimes(reference, day, hour, minute)?;

    candidates
        .iter()
        .copied()
        .filter(|candidate| *candidate >= reference)
        .min()
        .or_else(|| candidates.into_iter().max())
}

/// Builds the small candidate set used to disambiguate `DDHHMM` timestamps.
///
/// Invalid dates are discarded up front so downstream resolution code can compare only valid UTC
/// instants.
fn candidate_datetimes(
    reference: DateTime<Utc>,
    day: u32,
    hour: u32,
    minute: u32,
) -> Option<Vec<DateTime<Utc>>> {
    if day == 0 || day > 31 || hour > 23 || minute > 59 {
        return None;
    }

    let mut candidates = Vec::with_capacity(3);
    for (year, month) in surrounding_months(reference.year(), reference.month()) {
        if let Some(candidate) = build_candidate(year, month, day, hour, minute) {
            candidates.push(candidate);
        }
    }

    if candidates.is_empty() {
        None
    } else {
        Some(candidates)
    }
}

/// Returns the previous, current, and next month around the reference month.
fn surrounding_months(year: i32, month: u32) -> [(i32, u32); 3] {
    let previous = if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    };
    let next = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };

    [previous, (year, month), next]
}

/// Builds a UTC candidate only when the date and time components are valid together.
fn build_candidate(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
) -> Option<DateTime<Utc>> {
    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let time = NaiveTime::from_hms_opt(hour, minute, 0)?;
    Some(Utc.from_utc_datetime(&date.and_time(time)))
}
