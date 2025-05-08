use anyhow::anyhow;
use anyhow::Result;
use chrono::DateTime;
use chrono::Datelike;
use chrono::Local;
use chrono::NaiveDateTime;
use chrono::Utc;
use chrono::Weekday;
use chrono_tz::Tz;
use core::str;
use ical::{parser::ical::component::IcalEvent, property::Property as IcalProperty};
use std::str::FromStr;

pub(crate) fn parse_datetime(
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

#[derive(Debug, Clone)]
pub(crate) enum EventFrequency {
    Secondly,
    Minutely,
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl EventFrequency {
    pub(crate) fn from_str(s: &str) -> Result<Self> {
        match s {
            "SECONDLY" => Ok(Self::Secondly),
            "MINUTELY" => Ok(Self::Minutely),
            "HOURLY" => Ok(Self::Hourly),
            "DAILY" => Ok(Self::Daily),
            "WEEKLY" => Ok(Self::Weekly),
            "MONTHLY" => Ok(Self::Monthly),
            "YEARLY" => Ok(Self::Yearly),
            _ => Err(anyhow!("Unknown RRULE FREQ format: {}", s)),
        }
    }
}

pub(crate) struct EventIter {
    original_event: Event,
    last_start_dt: DateTime<Utc>,
    count: u32,
}

impl From<Event> for EventIter {
    fn from(event: Event) -> Self {
        let last_start_dt = event.start_dt.clone();
        Self {
            original_event: event,
            last_start_dt,
            count: 0,
        }
    }
}

impl EventIter {
    /// Cannot be BYMONTHDAY, BYYEARDAY, BYWEEKNO.
    ///
    /// BYDAY cannot specify a numeric value
    ///
    /// TODO: handle BYMONTH
    fn next_weekly(&self) -> Option<Event> {
        match &self.original_event.rrule {
            None => None,
            Some(rrule) => {
                let mut next_date = self.last_start_dt;
                loop {
                    next_date += chrono::Duration::days(1);
                    if next_date.weekday() == rrule.week_start {
                        next_date += chrono::Duration::days(7 * (rrule.interval - 1) as i64);
                    }
                    match &rrule.until {
                        Some(until_date) if next_date > *until_date => return None,
                        _ => {
                            let next_date_wd = next_date.weekday();
                            if rrule.byday.iter().any(|d| d.week_day == next_date_wd) {
                                let mut event = self.original_event.clone();
                                let diff = next_date - self.original_event.start_dt;
                                event.end_dt = self.original_event.end_dt + diff;
                                event.start_dt = next_date;
                                return Some(event);
                            }
                        }
                    }
                }
            }
        }
    }

    fn next_daily(&mut self) -> Option<Event> {
        // TODO
        None
    }

    fn next_monthly(&mut self) -> Option<Event> {
        // TODO
        None
    }

    fn next_yearly(&mut self) -> Option<Event> {
        // TODO
        None
    }
}

impl Iterator for EventIter {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        match self.count {
            0 => {
                self.count += 1;
                Some(self.original_event.clone())
            }
            _ => match &self.original_event.rrule {
                None => None,
                Some(RRule {
                    until: Some(until), ..
                }) if self.last_start_dt > *until => None,
                Some(RRule {
                    count: Some(count), ..
                }) if self.count >= *count => None,
                Some(rrule) => {
                    let next = match rrule.frequency {
                        EventFrequency::Daily => self.next_daily(),
                        EventFrequency::Weekly => self.next_weekly(),
                        EventFrequency::Monthly => self.next_monthly(),
                        EventFrequency::Yearly => self.next_yearly(),
                        _ => None, // TODO
                    };
                    if let Some(next) = next {
                        self.count += 1;
                        self.last_start_dt = next.start_dt;
                        Some(next)
                    } else {
                        None
                    }
                }
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ByDayDay {
    week_day: Weekday,
    n: Option<i32>,
}

fn parse_wd(s: &str) -> Result<Weekday> {
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

impl ByDayDay {
    fn parse(s: &str) -> Result<Self> {
        match s.len() {
            2 => Ok(Self {
                week_day: parse_wd(s)?,
                n: None,
            }),
            n if n > 2 => {
                let (n, wd) = s.split_at(s.len() - 2);
                let n = n.parse::<i32>()?;
                if n > 4 {
                    Err(anyhow!("Invalid BYDAY. Unexpected week number '{}'.", n))
                } else {
                    Ok(ByDayDay {
                        week_day: parse_wd(wd)?,
                        n: Some(n),
                    })
                }
            }
            _ => Err(anyhow!("Unsupported BYDAY format '{}'.", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RRule {
    frequency: EventFrequency,
    until: Option<DateTime<Utc>>,
    count: Option<u32>,
    interval: u32,
    week_start: Weekday,
    byday: Vec<ByDayDay>,

    #[allow(unused)]
    byweekno: Vec<i8>,
    #[allow(unused)]
    bymonth: Vec<u8>,
    #[allow(unused)]
    bymonthday: Vec<i8>,
    #[allow(unused)]
    byyearday: Vec<i16>,
}

/// RRULE:FREQ=WEEKLY;WKST=MO;UNTIL=20250707T070000Z;INTERVAL=1;BYDAY=MO,TU,WE,TH,FR
impl RRule {
    fn parse_frequency(s: &str, frequency: &mut Option<EventFrequency>) -> Result<()> {
        const NAME: &str = "FREQ";
        if frequency.is_some() {
            return Err(anyhow!(
                "Invalid RRULE: {} must be set exactly once '{}'",
                NAME,
                s
            ));
        }
        *frequency = Some(
            EventFrequency::from_str(s).map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?,
        );
        Ok(())
    }

    fn parse_until(s: &str, until: &mut Option<DateTime<Utc>>) -> Result<()> {
        const NAME: &str = "UNTIL";
        if until.is_some() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be set more than once: '{}'",
                NAME,
                s
            ));
        }
        *until =
            Some(parse_datetime(s, &None).map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?);
        Ok(())
    }

    fn parse_count(s: &str, count: &mut Option<u32>) -> Result<()> {
        const NAME: &str = "COUNT";
        if count.is_some() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be set more than once: '{}'",
                NAME,
                s
            ));
        }
        *count = Some(
            s.parse()
                .map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?,
        );
        Ok(())
    }

    fn parse_interval(s: &str, interval: &mut Option<u32>) -> Result<()> {
        const NAME: &str = "INTERVAL";
        if interval.is_some() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be set more than once: '{}'",
                NAME,
                s
            ));
        }
        *interval = {
            let interval = s
                .parse::<u32>()
                .map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?;
            if interval == 0 {
                return Err(anyhow!("Invalid RRULE: {} must not be zero: '{}'", NAME, s));
            }
            Some(interval)
        };
        Ok(())
    }

    fn parse_wkst(s: &str, week_start: &mut Option<Weekday>) -> Result<()> {
        const NAME: &str = "WKST";
        if week_start.is_some() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be set more than once: '{}'",
                NAME,
                s
            ));
        }
        *week_start = Some(parse_wd(s).map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?);
        Ok(())
    }

    fn parse_byday(s: &str, byday: &mut Vec<ByDayDay>) -> Result<()> {
        const NAME: &str = "BYDAY";
        if !byday.is_empty() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be set more than once: '{}'",
                NAME,
                s
            ));
        }
        for day in s.split(',').map(ByDayDay::parse) {
            let day = day.map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?;
            byday.push(day);
        }
        if byday.is_empty() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be empty: '{}'",
                NAME,
                s
            ));
        }
        Ok(())
    }

    fn parse_byweekno(s: &str, byweekno: &mut Vec<i8>) -> Result<()> {
        const NAME: &str = "BYWEEKNO";
        if !byweekno.is_empty() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be set more than once: '{}'",
                NAME,
                s
            ));
        }
        for m in s.split(',').map(i8::from_str) {
            let m = m.map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?;
            byweekno.push(m);
        }
        if byweekno.is_empty() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be empty: '{}'",
                NAME,
                s
            ));
        }
        Ok(())
    }

    /// Parses the BYMONTH rule part.
    ///
    /// According to RFC 5545 Section 3.3.10:
    /// The BYMONTH rule part specifies a COMMA-separated list of ordinals
    /// specifying months of the year. Valid values are 1 to 12.
    ///
    /// This rule part MUST NOT be specified more than once.
    /// If specified, the list MUST NOT be empty.
    ///
    /// # Arguments
    ///
    /// - `s` - The string value associated with the BYMONTH key (e.g., "1,2,3").
    /// - `bymonth` - A mutable reference to a vector where the parsed month
    ///    numbers (1-12) will be stored.
    ///
    /// After the function returns, the `bymonth` array is sorted in ascending order.
    ///
    /// # Errors
    ///
    /// Returns an `Err` if:
    /// - `bymonth` is already populated (i.e., BYMONTH is specified more than once).
    /// - `s` cannot be parsed into a list of valid month numbers (e.g., "JAN",
    ///   "0", "13", or non-numeric).
    /// - The parsed list of months is empty (e.g., `s` was an empty string,
    ///   though `split(',')` on "" yields [""]).
    fn parse_bymonth(s: &str, bymonth: &mut Vec<u8>) -> Result<()> {
        const NAME: &str = "BYMONTH";
        if !bymonth.is_empty() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be set more than once: '{}'",
                NAME,
                s
            ));
        }
        for m in s.split(',').map(u8::from_str) {
            let m = m.map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?;
            bymonth.push(m);
        }
        if bymonth.is_empty() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be empty: '{}'",
                NAME,
                s
            ));
        }
        Ok(())
    }

    fn parse_bymonthday(s: &str, bymonthday: &mut Vec<i8>) -> Result<()> {
        const NAME: &str = "BYWEEKNO";
        if !bymonthday.is_empty() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be set more than once: '{}'",
                NAME,
                s
            ));
        }
        for m in s.split(',').map(i8::from_str) {
            let m = m.map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?;
            bymonthday.push(m);
        }
        if bymonthday.is_empty() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be empty: '{}'",
                NAME,
                s
            ));
        }
        Ok(())
    }

    fn parse_byyearday(s: &str, byyearday: &mut Vec<i16>) -> Result<()> {
        const NAME: &str = "BYYEARDAY";
        if !byyearday.is_empty() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be set more than once: '{}'",
                NAME,
                s
            ));
        }
        for m in s.split(',').map(i16::from_str) {
            let m = m.map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?;
            byyearday.push(m);
        }
        if byyearday.is_empty() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be empty: '{}'",
                NAME,
                s
            ));
        }
        Ok(())
    }

    pub(crate) fn from_str(s: &str) -> Result<Self> {
        let mut frequency = None;
        let mut until = None;
        let mut count = None;
        let mut interval = None;
        let mut week_start = None;
        let mut byday = vec![];
        let mut byweekno = vec![];
        let mut bymonth = vec![];
        let mut bymonthday = vec![];
        let mut byyearday = vec![];

        for param in s.split(';') {
            let Some((name, value)) = param.split_once('=') else {
                return Err(anyhow!("Unexpected RRULE parameter: {}", param));
            };
            match name {
                "FREQ" => Self::parse_frequency(value, &mut frequency)?,
                "UNTIL" => Self::parse_until(value, &mut until)?,
                "COUNT" => Self::parse_count(value, &mut count)?,
                "INTERVAL" => Self::parse_interval(value, &mut interval)?,
                "WKST" => Self::parse_wkst(value, &mut week_start)?,
                "BYDAY" => Self::parse_byday(value, &mut byday)?,
                "BYWEEKNO" => Self::parse_byweekno(value, &mut byweekno)?,
                "BYMONTH" => Self::parse_bymonth(value, &mut bymonth)?,
                "BYMONTHDAY" => Self::parse_bymonthday(value, &mut bymonthday)?,
                "BYYEARDAY" => Self::parse_byyearday(value, &mut byyearday)?,
                _ => {}
            }
        }
        let week_start = week_start.unwrap_or(Weekday::Mon);
        byday.sort_by(|wd_l, wd_r| {
            if week_start == Weekday::Mon {
                wd_l.week_day
                    .number_from_monday()
                    .cmp(&wd_r.week_day.number_from_monday())
            } else {
                wd_l.week_day
                    .number_from_sunday()
                    .cmp(&wd_r.week_day.number_from_sunday())
            }
        });
        if until.is_some() && count.is_some() {
            return Err(anyhow!(
                "Invalid RRULE: UNTIL and COUNT are not allowed at the same time: '{}'",
                s
            ));
        }
        Ok(Self {
            frequency: frequency.ok_or(anyhow!("No FREQ param for RRULE"))?,
            until,
            count,
            interval: interval.unwrap_or(1),
            week_start,
            byday,
            byweekno,
            bymonth,
            bymonthday,
            byyearday,
        })
    }
}

#[derive(Clone)]
pub(crate) struct Event {
    pub(crate) uid: String,
    pub(crate) start_dt: DateTime<Utc>,
    pub(crate) end_dt: DateTime<Utc>,
    pub(crate) rrule: Option<RRule>,
    pub(crate) event: IcalEvent,

    #[allow(unused)]
    pub(crate) created_dt: DateTime<Utc>,
}

impl Event {
    pub(crate) fn recurring(&self) -> EventIter {
        EventIter::from(self.clone())
    }

    pub(crate) fn starts_within(
        &self,
        start_date: &Option<DateTime<Utc>>,
        end_date: &Option<DateTime<Utc>>,
    ) -> bool {
        match (start_date, end_date) {
            (None, None) => true,
            (Some(csd), None) => self.start_dt >= *csd,
            (None, Some(ced)) => self.start_dt <= *ced,
            (Some(csd), Some(ced)) => self.start_dt >= *csd && self.start_dt <= *ced,
        }
    }

    fn parse_uuid(prop: &IcalProperty) -> Result<String> {
        Ok(prop
            .value
            .as_ref()
            .ok_or(anyhow!("No value (datetime) for `UID` property"))?
            .clone())
    }

    fn parse_created(prop: &IcalProperty) -> Result<DateTime<Utc>> {
        let value = prop
            .value
            .as_ref()
            .ok_or(anyhow!("No value (datetime) for `CREATED` property"))?;
        let date = crate::ical::parse_datetime(value, &prop.params)
            .map_err(|e| anyhow!("Invalid ical date {prop:?}\n{e}"))?;
        Ok(date)
    }

    fn parse_dtend(prop: &IcalProperty) -> Result<DateTime<Utc>> {
        let value = prop
            .value
            .as_ref()
            .ok_or(anyhow!("No value (datetime) for `DTEND` property"))?;
        Ok(crate::ical::parse_datetime(value, &prop.params)
            .map_err(|e| anyhow!("Invalid ical date {prop:?}\n{e}"))?)
    }

    fn parse_dtstart(prop: &IcalProperty) -> Result<DateTime<Utc>> {
        let value = prop
            .value
            .as_ref()
            .ok_or(anyhow!("No value (datetime) for `DTSTART` property"))?;
        let date = crate::ical::parse_datetime(value, &prop.params)
            .map_err(|e| anyhow!("Invalid ical date {prop:?}\n{e}"))?;
        Ok(date)
    }

    fn parse_rrule(prop: &IcalProperty) -> Result<RRule> {
        let rrule = prop
            .value
            .as_ref()
            .ok_or(anyhow!("invalid RRULE: {}", prop.to_string()))?;
        Ok(RRule::from_str(rrule)?)
    }
}

impl TryFrom<IcalEvent> for Event {
    type Error = anyhow::Error;

    fn try_from(event: IcalEvent) -> Result<Self> {
        let mut start_dt = None;
        let mut end_dt = None;
        let mut created_dt = None;
        let mut uid = None;
        let mut rrule = None;
        for prop in event.properties.iter() {
            match prop.name.as_str() {
                "DTSTART" => start_dt = Some(Self::parse_dtstart(prop)?),
                "DTEND" => end_dt = Some(Self::parse_dtend(prop)?),
                "CREATED" => created_dt = Some(Self::parse_created(prop)?),
                "UID" => uid = Some(Self::parse_uuid(prop)?),
                "RRULE" => rrule = Some(Self::parse_rrule(prop)?),
                _ => {}
            }
        }
        Ok(Self {
            start_dt: start_dt.ok_or(anyhow!(
                "Unsupported event: no DTSTART. Event: UID={:?} CREATED={:?}",
                uid,
                created_dt
            ))?,
            end_dt: end_dt.ok_or(anyhow!(
                "Unsupported event: no DTEND. Event: UID={:?} CREATED={:?}",
                uid,
                created_dt
            ))?,
            created_dt: created_dt.ok_or(anyhow!(
                "Unsupported event: no CREATED. Event: UID={:?} DTSTART={:?}",
                uid,
                start_dt
            ))?,
            uid: uid.ok_or(anyhow!(
                "Unsupported event: no UID. Event: DTSTART={:?} CREATED={:?}",
                start_dt,
                created_dt
            ))?,
            event,
            rrule,
        })
    }
}
