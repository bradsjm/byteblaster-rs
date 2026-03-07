//! Time resolution utilities for WMO headers.
//!
//! This module provides functions for resolving ambiguous date/time references in WMO headers,
//! where only day-of-month is provided (not month/year). These utilities use a reference time
//! to determine the most likely intended date.

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Utc};

/// Resolves a day/hour/minute reference to the nearest plausible DateTime relative to a reference.
///
/// This function considers candidates in the surrounding months and returns the one closest
/// to the reference time. Used to resolve DDHHMM fields in WMO headers where only the day
/// of month is provided.
///
/// # Arguments
///
/// * `reference` - The reference UTC time (typically current time)
/// * `day` - Day of month (1-31)
/// * `hour` - Hour (0-23)
/// * `minute` - Minute (0-59)
///
/// # Returns
///
/// The `DateTime<Utc>` closest to reference, or `None` if inputs are invalid
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

/// Resolves a clock time (hour/minute) to the nearest DateTime relative to a reference.
///
/// Similar to `resolve_day_time_nearest` but only uses hour and minute. Considers
/// candidates from yesterday, today, and tomorrow relative to the reference date.
///
/// # Arguments
///
/// * `reference` - The reference UTC time
/// * `hour` - Hour (0-23)
/// * `minute` - Minute (0-59)
///
/// # Returns
///
/// The `DateTime<Utc>` closest to reference, or `None` if inputs are invalid
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

/// Resolves a day/hour/minute reference to a DateTime that is not before the reference time.
///
/// This function returns the earliest candidate that is on or after the reference time.
/// If all candidates are before the reference, returns the latest candidate (most recent).
/// Useful for determining issuance times that should not be in the future.
///
/// # Arguments
///
/// * `reference` - The reference UTC time
/// * `day` - Day of month (1-31)
/// * `hour` - Hour (0-23)
/// * `minute` - Minute (0-59)
///
/// # Returns
///
/// The earliest `DateTime<Utc>` not before reference, or `None` if inputs are invalid
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

/// Generates candidate DateTimes in the surrounding months for a given day/time.
///
/// Creates up to 3 candidates: one in the previous month, current month, and next month.
/// Filters out invalid dates (e.g., day 31 in a 30-day month).
///
/// # Arguments
///
/// * `reference` - Used to determine the year and month context
/// * `day` - Day of month (1-31)
/// * `hour` - Hour (0-23)
/// * `minute` - Minute (0-59)
///
/// # Returns
///
/// A vector of valid candidates, or `None` if no valid dates could be constructed
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

/// Returns the previous, current, and next month relative to the given year/month.
///
/// Handles year boundaries correctly (e.g., month 1 returns December of previous year).
///
/// # Arguments
///
/// * `year` - The reference year
/// * `month` - The reference month (1-12)
///
/// # Returns
///
/// Array of 3 (year, month) tuples: [previous, current, next]
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

/// Constructs a UTC DateTime from year, month, day, hour, and minute components.
///
/// # Arguments
///
/// * `year` - Year (e.g., 2025)
/// * `month` - Month (1-12)
/// * `day` - Day of month (1-31)
/// * `hour` - Hour (0-23)
/// * `minute` - Minute (0-59)
///
/// # Returns
///
/// `Some(DateTime<Utc>)` if the components form a valid date/time, `None` otherwise
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
