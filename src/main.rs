mod config;
mod harvest;
mod ical;

use crate::config::Config;
use ::ical::{parser::ical::component::IcalEvent, IcalParser};
use anyhow::{anyhow, Result};
use chrono::Local;
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, Write};
use std::os::fd::{AsRawFd, FromRawFd};

struct Work {
    pub(crate) inner: harvest::Work,
    pub(crate) props: Vec<Option<String>>,
}

impl Work {
    fn from_ical_event_props(
        event_props: &[::ical::property::Property],
        config: &Config,
    ) -> Result<Option<Self>> {
        let n_extra_props = config.extra_props.len();
        let mut props = Vec::<Option<String>>::with_capacity(n_extra_props);
        props.resize(n_extra_props, None);
        let mut work = harvest::Work::new(
            config.first_name.clone(),
            config.last_name.clone(),
            config.default_task.clone(),
        );
        let mut attendeies = HashSet::new();
        let accepted_state_name = "ACCEPTED".to_string();
        for prop in event_props.iter() {
            match prop.name.as_str() {
                "ORGANIZER" => {
                    if !config.required_attendies.is_empty() {
                        if let Some(value) = &prop.value {
                            attendeies.insert(value.clone());
                        }
                    }
                }
                "ATTENDEE" => {
                    if !config.required_attendies.is_empty() {
                        if let Some(value) = &prop.value {
                            if let Some(params) = &prop.params {
                                if params
                                    .iter()
                                    .find(|p| {
                                        p.0 == "PARTSTAT" && p.1.contains(&accepted_state_name)
                                    })
                                    .is_some()
                                {
                                    attendeies.insert(value.clone());
                                }
                            }
                        }
                    }
                }
                "SUMMARY" => work.notes = prop.value.clone(),
                "DTSTART" => {
                    let value = prop
                        .value
                        .as_ref()
                        .ok_or(anyhow!("No value (datetime) for `DTSTART` property"))?;
                    let date = crate::ical::parse_datetime(value, &prop.params)
                        .map_err(|e| anyhow!("Invalid ical date {prop:?}\n{e}"))?;
                    work.start_datetime = Some(date);
                }
                "DTEND" => {
                    let value = prop
                        .value
                        .as_ref()
                        .ok_or(anyhow!("No value (datetime) for `DTSTART` property"))?;
                    let date = crate::ical::parse_datetime(value, &prop.params)
                        .map_err(|e| anyhow!("Invalid ical date {prop:?}\n{e}"))?;
                    work.end_datetime = Some(date);
                }
                name => {
                    if let Some(i) = config.extra_props.iter().position(|k| k.as_str() == name) {
                        props[i] = prop.value.clone();
                    }
                }
            }
        }
        if attendeies.is_empty() || config.required_attendies.is_subset(&attendeies) {
            Ok(Some(Self { inner: work, props }))
        } else {
            Ok(None)
        }
    }
}

fn log_work<IO: Write>(work: &Work, file: &mut csv::Writer<IO>) -> Result<()> {
    let props = &work.props;
    let work = &work.inner;
    let hours = work.hours();
    let hours = hours.unwrap_or("0".into());
    let date = work.date_string();
    let notes = work
        .notes
        .as_ref()
        .map(|s| s.replace(r"\,", ","))
        .map(|s| s.replace(r"\;", ";"))
        .unwrap_or("".into());
    let required_values = vec![
        date.as_ref()
            .ok_or(anyhow!("The work has no date\n{work:?}"))?
            .as_str(),
        work.task.client.as_str(),
        work.task.project.as_str(),
        work.task.project_code.as_str(),
        work.task.name.as_str(),
        notes.as_str(),
        hours.as_str(),
        work.first_name.as_str(),
        work.last_name.as_str(),
    ];
    let empty_string = String::new();
    let record = props
        .iter()
        .map(|p| p.as_ref().unwrap_or_else(|| &empty_string).as_str())
        .chain(required_values.into_iter())
        .collect::<Vec<_>>();
    file.write_record(&record)?;
    //eprintln!("{record:?}");
    Ok(())
}

fn announce_event_collection(config: &Config) {
    let start_date = &config
        .start_date
        .map(|dt| {
            " from ".to_string()
                + &dt.with_timezone(&Local).date_naive().to_string()
                + " (inclusive)"
        })
        .unwrap_or("".into());
    let end_date = &config
        .end_date
        .map(|dt| {
            " to ".to_string() + &dt.with_timezone(&Local).date_naive().to_string() + " (exclusive)"
        })
        .unwrap_or("".into());

    eprintln!("Collecting events{start_date}{end_date} ...");
}

fn open_ical_reader(config: &Config) -> Result<IcalParser<BufReader<File>>> {
    let file = if let Some(path) = config.input.as_ref() {
        File::open(path.clone())
            .map_err(|e| anyhow!("Cannot open the intput file {path:?}\n{e}"))?
    } else {
        unsafe { File::from_raw_fd(io::stdin().as_raw_fd()) }
    };

    let file_reader = BufReader::new(file);
    Ok(IcalParser::new(file_reader))
}

fn open_csv_writer(config: &Config) -> Result<csv::Writer<File>> {
    let file = if let Some(path) = &config.output {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .append(false)
            .open(path.clone())
            .map_err(|e| anyhow!("Cannot open the output file {path:?}\n{e}"))?
    } else {
        unsafe { File::from_raw_fd(io::stdout().as_raw_fd()) }
    };

    Ok(csv::WriterBuilder::new().from_writer(file))
}

fn process_event(event: &IcalEvent, config: &Config) -> Result<Option<Work>> {
    let patterns = &config.tasks;
    let work = Work::from_ical_event_props(&event.properties, &config)?;
    let Some(mut work) = work else {
        return Ok(None);
    };
    if work
        .inner
        .starts_within(&config.start_date, &config.end_date)
    {
        if let Some(pattern) = patterns
            .iter()
            .filter(|p| {
                work.inner
                    .notes
                    .as_ref()
                    .map(|s| p.regex.is_match(&s))
                    .unwrap_or(false)
            })
            .next()
        {
            work.inner.task = pattern.task.clone();
        }
        Ok(Some(work))
    } else {
        Ok(None)
    }
}

fn main() -> Result<()> {
    let config = config::config();
    //eprintln!("{config:?}");

    let ical_reader = open_ical_reader(&config)?;
    let mut csv_writer = open_csv_writer(&config)?;

    let column_names = config
        .extra_props
        .iter()
        .map(String::as_str)
        .chain(harvest::REQUIRED_CSV_COLUMN_NAMES.iter().cloned());
    csv_writer
        .write_record(column_names)
        .map_err(|e| anyhow!("Cannot write the CSV headers to the output file: {e}"))?;

    announce_event_collection(&config);

    let mut events_collected = 0;
    for calendar in ical_reader {
        let calendar = calendar?;
        for event in calendar.events {
            if let Some(work) = process_event(&event, &config)? {
                log_work(&work, &mut csv_writer).map_err(|e| anyhow!("Cannot log work\n{e}"))?;
                events_collected += 1;
            }
        }
    }

    csv_writer
        .flush()
        .map_err(|e| anyhow!("Cannot write to the output file\n{e}"))?;

    eprintln!();
    eprintln!("Events collected. Events total: {events_collected}");

    Ok(())
}
