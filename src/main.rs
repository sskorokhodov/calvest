mod config;
mod harvest;
mod ical;

use crate::config::Config;
use crate::ical::Event;
use ::ical::{parser::ical::component::IcalEvent, IcalParser};
use anyhow::{anyhow, Result};
use chrono::{NaiveDate, Utc};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, Write};
use std::os::fd::{AsRawFd, FromRawFd};

#[derive(Debug, Clone)]
struct Work {
    pub(crate) inner: harvest::Work,
    pub(crate) props: Vec<Option<String>>,
}

impl Work {
    fn from_event(event: &Event, config: &Config) -> Result<Option<Self>> {
        let n_extra_props = config.extra_props.len();
        let mut props = Vec::<Option<String>>::with_capacity(n_extra_props);
        props.resize(n_extra_props, None);
        let mut work = harvest::Work::new(
            config.first_name.clone(),
            config.last_name.clone(),
            config.default_task.clone(),
        );
        work.start_datetime = Some(event.start_dt.clone());
        work.end_datetime = Some(event.end_dt.clone());
        let mut attendeies = HashSet::new();
        let accepted_state_name = "ACCEPTED".to_string();
        for prop in event.event.properties.iter() {
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
                + &dt.with_timezone(&chrono::Local).date_naive().to_string()
                + " (inclusive)"
        })
        .unwrap_or("".into());
    let end_date = &config
        .end_date
        .map(|dt| {
            " to ".to_string()
                + &dt.with_timezone(&chrono::Local).date_naive().to_string()
                + " (exclusive)"
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

fn relevant_events(event: &IcalEvent, config: &Config) -> Result<Vec<Event>> {
    //eprintln!();
    let Some(_summary) = event.properties.iter().find(|p| p.name == "SUMMARY") else {
        //eprintln!("WARN: No SUMMARY: {event:?}");
        return Ok(vec![]);
    };
    //eprintln!("Processing event: {}", summary.value.as_ref().unwrap());
    let event = match Event::try_from(event.clone()) {
        Ok(event) => event,
        Err(error) => {
            eprintln!("WARN: {error}");
            return Ok(vec![]);
        }
    };
    //eprintln!("  rrule: {:?}", event.rrule);
    let until_date = config.end_date.unwrap_or(Utc::now());
    let from_date = &config.start_date;
    Ok(event
        .recurring()
        .filter(|event| event.starts_within(&from_date, &Some(until_date)))
        .collect())
}

fn event_to_work(event: &Event, config: &Config) -> Result<Option<Work>> {
    let patterns = &config.tasks;
    let Some(mut work) = Work::from_event(&event, &config)? else {
        return Ok(None);
    };
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

    let mut events = vec![];
    for calendar in ical_reader {
        let calendar = calendar?;
        for event in calendar.events {
            let mut event_chain = relevant_events(&event, &config)?;
            events.append(&mut event_chain);
        }
    }

    let mut deduplicated_events: HashMap<NaiveDate, HashMap<String, Event>> = HashMap::new();
    for event in events {
        if let Some(events) = deduplicated_events.get_mut(&event.start_dt.date_naive()) {
            events.insert(event.uid.clone(), event);
        } else {
            let mut events = HashMap::new();
            let start_dt = event.start_dt.clone();
            events.insert(event.uid.clone(), event);
            deduplicated_events.insert(start_dt.date_naive(), events);
        }
    }

    let mut work_entries = 0;
    for event in deduplicated_events
        .values()
        .map(|events| events.values())
        .flatten()
    {
        if let Some(work) = event_to_work(&event, &config)? {
            log_work(&work, &mut csv_writer).map_err(|e| anyhow!("Cannot log work\n{e}"))?;
            work_entries += 1;
        }
    }

    csv_writer
        .flush()
        .map_err(|e| anyhow!("Cannot write to the output file\n{e}"))?;

    eprintln!();
    eprintln!("Events collected. Work entries total: {work_entries}");

    Ok(())
}
