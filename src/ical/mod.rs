mod parse;
mod rrule;

use anyhow::anyhow;
use anyhow::Result;
use chrono::DateTime;
use chrono::Datelike;
use chrono::Months;
use chrono::Utc;
use ical::{parser::ical::component::IcalEvent, property::Property as IcalProperty};

use rrule::EventFrequency;
use rrule::RRule;

pub struct EventIter {
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
                            if rrule.byday_matches(&next_date) {
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
        eprintln!(
            "WARN: unsupported event frequency: DAILY. Event: {:?}",
            self.original_event.event.summary().unwrap_or_default()
        );
        // TODO
        None
    }

    fn next_monthly(&mut self) -> Option<Event> {
        match &self.original_event.rrule {
            None => None,
            Some(rrule) => {
                // interval
                // (?) bymonth
                // bymonthday
                // byday
                // (unsupported) bysetpos
                if !rrule.bymonth.is_empty() {
                    eprintln!(
                        "WARN: unsupported MONTHLY event RRULE: BYMONTH is not supported. Event: {:?}",
                        self.original_event.event.summary().unwrap_or_default()
                    );
                    return None;
                }
                if !rrule.bysetpos.is_empty() {
                    eprintln!(
                        "WARN: unsupported MONTHLY event RRULE: BYSETPOS not supported. Event: {:?}",
                        self.original_event.event.summary().unwrap_or_default()
                    );
                    return None;
                }
                let mut next_date = self.last_start_dt;
                loop {
                    next_date += chrono::Duration::days(1);
                    if next_date.day() == 1 {
                        next_date = next_date
                            .checked_add_months(Months::new(rrule.interval - 1))
                            .unwrap();
                    }
                    match &rrule.until {
                        Some(until_date) if next_date > *until_date => return None,
                        _ => {
                            if rrule.bymonthday_matches(&next_date)
                                && rrule.byday_matches(&next_date)
                            {
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

    fn next_yearly(&mut self) -> Option<Event> {
        // TODO
        eprintln!(
            "WARN: unsupported event frequency: YEARLY. Event: {:?}",
            self.original_event.event.summary().unwrap_or_default()
        );
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
                    let next = match &rrule.frequency {
                        &EventFrequency::Daily => self.next_daily(),
                        &EventFrequency::Weekly => self.next_weekly(),
                        &EventFrequency::Monthly => self.next_monthly(),
                        &EventFrequency::Yearly => self.next_yearly(),
                        freq => {
                            eprintln!(
                                "WARN: unsupported event frequency: {:?}. Event: {:?}",
                                freq,
                                self.original_event.event.summary().unwrap_or_default()
                            );
                            None // TODO
                        }
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

pub trait Summary {
    fn summary(&self) -> Option<String>;
}

impl Summary for IcalEvent {
    fn summary(&self) -> Option<String> {
        self.properties
            .iter()
            .find(|p| p.name.to_uppercase() == "SUMMARY")
            .map(|p| p.value.clone())
            .flatten()
    }
}

pub trait StartDate {
    fn start_date(&self) -> Option<DateTime<Utc>>;
}

impl StartDate for IcalEvent {
    fn start_date(&self) -> Option<DateTime<Utc>> {
        self.properties
            .iter()
            .find(|p| p.name.to_uppercase() == "DTSTART")
            .map(|p| Event::parse_dtstart(p).ok())
            .flatten()
    }
}

#[derive(Clone)]
pub struct Event {
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

    #[allow(unused)]
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
        let date = parse::datetime(value, &prop.params)
            .map_err(|e| anyhow!("Invalid ical date {prop:?}\n{e}"))?;
        Ok(date)
    }

    fn parse_dtend(prop: &IcalProperty) -> Result<DateTime<Utc>> {
        let value = prop
            .value
            .as_ref()
            .ok_or(anyhow!("No value (datetime) for `DTEND` property"))?;
        Ok(parse::datetime(value, &prop.params)
            .map_err(|e| anyhow!("Invalid ical date {prop:?}\n{e}"))?)
    }

    fn parse_dtstart(prop: &IcalProperty) -> Result<DateTime<Utc>> {
        let value = prop
            .value
            .as_ref()
            .ok_or(anyhow!("No value (datetime) for `DTSTART` property"))?;
        let date = parse::datetime(value, &prop.params)
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
