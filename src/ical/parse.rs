use anyhow::anyhow;
use anyhow::Result;
use chrono::DateTime;
use chrono::Local;
use chrono::NaiveDateTime;
use chrono::Utc;
use chrono::Weekday;
use chrono_tz::Tz;
use core::str;

pub(crate) fn datetime(
    s: &str,
    params: &Option<Vec<(String, Vec<String>)>>,
) -> Result<DateTime<Utc>> {
    let is_date = params
        .as_ref()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .find(|(n, v)| {
            n.to_uppercase().as_str() == "VALUE"
                && v.first()
                    .map(|v| v.to_uppercase().as_str() == "DATE")
                    .unwrap_or(false)
        })
        .is_some();
    let datetime_s = if is_date {
        s.to_string() + "T000000"
    } else {
        s.split_at(15).0.to_string()
    };
    let tzid = params
        .as_ref()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .find(|(n, _)| n.to_uppercase().as_str() == "TZID")
        .map(|p| p.1.first())
        .flatten();
    let tz = tzid.map(|tzid| tzid.parse::<Tz>());
    let datetime = match tz {
        Some(tz) => NaiveDateTime::parse_from_str(&datetime_s, "%Y%m%dT%H%M%S")?
            .and_local_timezone(tz?)
            .unwrap()
            .to_utc(),
        None => NaiveDateTime::parse_from_str(&datetime_s, "%Y%m%dT%H%M%S")?
            .and_local_timezone(Local)
            .unwrap()
            .to_utc(),
    };
    Ok(datetime)
}

pub(crate) fn week_day(s: &str) -> Result<Weekday> {
    match s {
        "MO" => Ok(Weekday::Mon),
        "TU" => Ok(Weekday::Tue),
        "WE" => Ok(Weekday::Wed),
        "TH" => Ok(Weekday::Thu),
        "FR" => Ok(Weekday::Fri),
        "SA" => Ok(Weekday::Sat),
        "SU" => Ok(Weekday::Sun),
        _ => Err(anyhow!("Unsupported BYDAY {}", s)),
    }
}
