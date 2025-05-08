use crate::ical::parse;
use anyhow::anyhow;
use anyhow::Result;
use chrono::DateTime;
use chrono::Datelike;
use chrono::Month;
use chrono::Utc;
use chrono::Weekday;
use core::str;
use std::str::FromStr;

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

#[derive(Clone, Debug, PartialEq)]
pub struct ByMonthDayDay {
    pub month_day: i8,
}

impl ByMonthDayDay {
    fn matches(&self, dt: &DateTime<Utc>) -> bool {
        if self.month_day > 0 {
            dt.day() as i8 == self.month_day
        } else {
            let month_days = Month::try_from(dt.month() as u8)
                .unwrap()
                .num_days(dt.year())
                .unwrap();
            self.month_day.abs() as u8 == ((month_days - dt.day() as u8) / 7) + 1
        }
    }
}

impl TryFrom<i8> for ByMonthDayDay {
    type Error = anyhow::Error;
    fn try_from(value: i8) -> std::result::Result<Self, Self::Error> {
        if value > 31 || value < -31 || value == 0 {
            return Err(anyhow!("Invalud BYMONTHDAY value: {}", value));
        }
        Ok(Self { month_day: value })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ByDayDay {
    pub week_day: Weekday,
    pub n: Option<i32>,
}

impl ByDayDay {
    fn parse(s: &str) -> Result<Self> {
        match s.len() {
            2 => Ok(Self {
                week_day: parse::week_day(s)?,
                n: None,
            }),
            n if n > 2 => {
                let (n, wd) = s.split_at(s.len() - 2);
                let n = n.parse::<i32>()?;
                if n > 4 || n == 0 {
                    Err(anyhow!("Invalid BYDAY. Unexpected week number '{}'.", n))
                } else {
                    Ok(ByDayDay {
                        week_day: parse::week_day(wd)?,
                        n: Some(n),
                    })
                }
            }
            _ => Err(anyhow!("Unsupported BYDAY format '{}'.", s)),
        }
    }

    fn matches(&self, dt: &DateTime<Utc>) -> bool {
        if dt.weekday() == self.week_day {
            if let Some(n) = self.n {
                if n > 0 {
                    n.abs() as u8 == (dt.day() as u8 / 7) + 1
                } else {
                    let month_days = Month::try_from(dt.month() as u8)
                        .unwrap()
                        .num_days(dt.year())
                        .unwrap();
                    n.abs() as u8 == ((month_days - dt.day() as u8) / 7) + 1
                }
            } else {
                true
            }
        } else {
            false
        }
    }
}
#[derive(Debug, Clone)]
pub struct RRule {
    pub frequency: EventFrequency,
    pub until: Option<DateTime<Utc>>,
    pub count: Option<u32>,
    pub interval: u32,
    pub week_start: Weekday,
    pub byday: Vec<ByDayDay>,
    pub bymonthday: Vec<ByMonthDayDay>,

    #[allow(unused)]
    pub byweekno: Vec<i8>,
    #[allow(unused)]
    pub bymonth: Vec<u8>,
    #[allow(unused)]
    pub byyearday: Vec<i16>,
    #[allow(unused)]
    pub bysetpos: Vec<i16>,
}

const ORDYRNUM_MAX: u16 = 366;

/// RRULE:FREQ=WEEKLY;WKST=MO;UNTIL=20250707T070000Z;INTERVAL=1;BYDAY=MO,TU,WE,TH,FR
impl RRule {
    pub fn byday_matches(&self, dt: &DateTime<Utc>) -> bool {
        self.byday.is_empty() || self.byday.iter().any(|d| d.matches(&dt))
    }

    pub fn bymonthday_matches(&self, dt: &DateTime<Utc>) -> bool {
        self.bymonthday.is_empty() || self.bymonthday.iter().any(|d| d.matches(&dt))
    }

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
        *until = Some(
            parse::datetime(s, &None).map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?,
        );
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
        *week_start =
            Some(parse::week_day(s).map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?);
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
        bymonth.sort();
        Ok(())
    }

    fn parse_bymonthday(s: &str, bymonthday: &mut Vec<ByMonthDayDay>) -> Result<()> {
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
            bymonthday.push(m.try_into()?);
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
        for d in s.split(',').map(i16::from_str) {
            let d = d.map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?;
            if d.abs() > ORDYRNUM_MAX as i16 {
                return Err(anyhow!(
                    "Invalid {} '{}': absolute value must be <= {}",
                    NAME,
                    s,
                    ORDYRNUM_MAX
                ));
            }
            byyearday.push(d);
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

    fn parse_bysetpos(s: &str, bysetpos: &mut Vec<i16>) -> Result<()> {
        const NAME: &str = "BYSETPOS";
        if !bysetpos.is_empty() {
            return Err(anyhow!(
                "Invalid RRULE: {} must not be set more than once: '{}'",
                NAME,
                s
            ));
        }
        for d in s.split(',').map(i16::from_str) {
            let d = d.map_err(|e| anyhow!("Invalid {} '{}': {}", NAME, s, e))?;
            if d.abs() > ORDYRNUM_MAX as i16 {
                return Err(anyhow!(
                    "Invalid {} '{}': absolute value must be <= {}",
                    NAME,
                    s,
                    ORDYRNUM_MAX
                ));
            }
            bysetpos.push(d);
        }
        if bysetpos.is_empty() {
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
        let mut bysetpos = vec![];

        for param in s.split(';') {
            let Some((name, value)) = param.split_once('=') else {
                return Err(anyhow!("Unexpected RRULE parameter: {}", param));
            };
            match name.to_uppercase().as_str() {
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
                "BYSETPOS" => Self::parse_bysetpos(value, &mut bysetpos)?,
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
            bysetpos,
        })
    }
}
