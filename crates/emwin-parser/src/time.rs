use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Utc};

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
