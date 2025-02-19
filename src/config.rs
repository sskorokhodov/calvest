use crate::harvest::Task;
use chrono::DateTime;
use chrono::Datelike;
use chrono::Days;
use chrono::Local;
use chrono::Months;
use chrono::NaiveDateTime;
use chrono::NaiveTime;
use chrono::Utc;
use clap::builder::NonEmptyStringValueParser;
use clap::Arg;
use clap::ArgAction;
use clap::Command;
use clap::ValueEnum;
use clap_complete::Shell;
use regex::Regex;
use std::path::PathBuf;

fn wrap_at<S: ToString>(s: S, at: usize) -> String {
    let s = s.to_string();
    let words = s.split(&[' ', '\t']).filter(|l| !l.is_empty());
    let mut wrapped = vec![];
    let mut line = String::new();
    for w in words {
        if !line.is_empty() && line.len() + w.len() >= at {
            wrapped.push(line);
            line = "".into()
        }
        line = line + w + " ";
    }
    wrapped.push(line);
    wrapped.join("\n")
}

fn wrap_help<S: ToString>(s: S) -> String {
    wrap_at(s, 70)
}

#[derive(Debug)]
pub(crate) struct TaskPattern {
    pub(crate) task: Task,
    pub(crate) regex: Regex,
}

#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) input: Option<PathBuf>,
    pub(crate) output: Option<PathBuf>,
    pub(crate) extra_props: Vec<String>,
    pub(crate) first_name: String,
    pub(crate) last_name: String,
    pub(crate) default_task: Task,
    pub(crate) start_date: Option<DateTime<Utc>>,
    pub(crate) end_date: Option<DateTime<Utc>>,
    pub(crate) tasks: Vec<TaskPattern>,
}

fn date_str_to_datetime(s: &str) -> Result<DateTime<Utc>, String> {
    Ok(
        NaiveDateTime::parse_from_str(&(s.to_string() + "000000"), "%Y-%m-%d%H%M%S")
            .map_err(|e| e.to_string())?
            .and_utc(),
    )
}

#[derive(ValueEnum, Clone)]
enum Period {
    LastMonth,
    ThisMonth,
}

fn current_month_start() -> DateTime<Utc> {
    Local::now()
        .with_day(1)
        .unwrap()
        .with_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap())
        .unwrap()
        .to_utc()
}

fn cli() -> clap::Command {
    Command::new("calvest - iCal to Harvest (CSV) transformer")
        .author(clap::crate_authors!())
        .version(clap::crate_version!())
        .long_version(clap::crate_version!())
        .about(clap::crate_description!())
        .args([
            Arg::new("print-completions")
                .long("print-completions")
                .value_name("SHELL")
                .help("Print shell completions.")
                .value_parser(clap::value_parser!(clap_complete::Shell)),
            Arg::new("input")
                .long("input")
                .value_name("FILE")
                .help("Read the YAML from <FILE> instead of <stdin>.")
                .value_parser(clap::value_parser!(PathBuf))
                .required_unless_present("print-completions")
                .num_args(1),
            Arg::new("output")
                .long("output")
                .value_name("FILE")
                .help("Write the result into the <FILE> instead of printing to <stdout>.")
                .value_parser(clap::value_parser!(PathBuf))
                .required_unless_present("print-completions")
                .num_args(1),
            Arg::new("include-property")
                .long("include-property")
                .value_name("PROPERTY_NAME")
                .help(wrap_help(
                    [
                        "Additional property to include into the CSV.",
                        "The property name becomes the column name.",
                    ]
                    .join(" "),
                ))
                .action(ArgAction::Set)
                .num_args(1),
            Arg::new("task")
                .long("task")
                .value_names(&["TASK_NAME", "PROJECT_NAME", "CLIENT_NAME", "REGEX"])
                .value_parser(NonEmptyStringValueParser::new())
                .action(ArgAction::Append)
                .num_args(4)
                .help("Use these task, project, client when the event summary matches the regex."),
            Arg::new("start-date")
                .long("start-date")
                .value_name("START_DATE")
                .value_parser(date_str_to_datetime)
                .num_args(1)
                .help("Set the start date for filtering events."),
            Arg::new("end-date")
                .long("end-date")
                .value_name("END_DATE")
                .value_parser(date_str_to_datetime)
                .num_args(1)
                .help("Set the end date for filtering events."),
            Arg::new("period")
                .long("timeframe")
                .alias("period")
                .conflicts_with_all(["start-date", "end-date"])
                .value_name("PERIOD")
                .value_parser(clap::value_parser!(Period))
                .num_args(1)
                .help("Set the end date for filtering events."),
            Arg::new("default-task")
                .long("default-task")
                .value_names(&["TASK_NAME", "PROJECT_NAME", "CLIENT_NAME"])
                .action(ArgAction::Append)
                .value_parser(NonEmptyStringValueParser::new())
                .num_args(3)
                .required_unless_present("print-completions")
                .help("Set the default task with the task name."),
            Arg::new("first-name")
                .long("first-name")
                .value_name("FIRST_NAME")
                .num_args(1)
                .value_parser(NonEmptyStringValueParser::new())
                .required_unless_present("print-completions")
                .help("Set the employe first name."),
            Arg::new("last-name")
                .long("last-name")
                .value_name("LAST_NAME")
                .value_parser(NonEmptyStringValueParser::new())
                .num_args(1)
                .required_unless_present("print-completions")
                .help("Set the employe last name."),
        ])
}

pub(crate) fn config() -> Config {
    let matches = cli().get_matches();

    if let Some(shell) = matches.get_one::<Shell>("print-completions").copied() {
        let mut cmd = cli();
        eprintln!("Generating completion file for {shell}...");
        let name = cmd.get_name().to_string();
        clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
        std::process::exit(0);
    }

    let (start_date, end_date) = match matches.get_one::<Period>("period").cloned() {
        Some(Period::LastMonth) => {
            let start_date = current_month_start()
                .checked_sub_months(Months::new(1))
                .unwrap();
            let end_date = current_month_start();
            (Some(start_date), Some(end_date))
        }
        Some(Period::ThisMonth) => {
            let start_date = current_month_start();
            let end_date = current_month_start()
                .checked_add_months(Months::new(1))
                .unwrap();
            (Some(start_date), Some(end_date))
        }
        None => (
            matches.get_one::<DateTime<Utc>>("start-date").cloned(),
            matches
                .get_one::<DateTime<Utc>>("end-date")
                .cloned()
                .map(|d| d.checked_add_days(Days::new(1)).unwrap_or(d).to_utc()),
        ),
    };

    let config = Config {
        output: matches.get_one::<PathBuf>("output").map(Clone::clone),
        input: matches.get_one::<PathBuf>("input").map(Clone::clone),
        extra_props: matches
            .get_many::<String>("include-property")
            .unwrap_or_default()
            .map(Clone::clone)
            .collect(),
        first_name: matches.get_one::<String>("first-name").unwrap().clone(),
        last_name: matches.get_one::<String>("last-name").unwrap().clone(),
        default_task: matches
            .get_many::<String>("default-task")
            .map(|c| {
                let c = c.collect::<Vec<_>>();
                Task {
                    name: c[0].clone(),
                    project: c[1].clone(),
                    client: c[2].clone(),
                }
            })
            .unwrap(),
        start_date,
        end_date,
        tasks: matches
            .get_many::<String>("task")
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<&String>>()
            .chunks(4)
            .map(|c| {
                let task = Task {
                    name: c[0].clone(),
                    project: c[1].clone(),
                    client: c[2].clone(),
                };
                TaskPattern {
                    task,
                    // TODO parse during the config parsing
                    regex: Regex::new(c[3]).unwrap(),
                }
            })
            .collect(),
    };
    config
}
