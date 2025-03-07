use chrono::DateTime;
use chrono::Utc;

/// Date (YYYY-MM-DD or M/D/YYYY formats; for example: 2023-08-25 or 8/25/2023)
/// Hours (In decimal format, without any stray characters; for example: 7.5, 3, 9.9)
pub(crate) const REQUIRED_CSV_COLUMN_NAMES: &[&str] = &[
    "Date",
    "Client",
    "Project",
    "Project Code",
    "Task",
    "Notes",
    "Hours",
    "First name",
    "Last name",
];

#[derive(Debug, Clone)]
pub(crate) struct Task {
    pub(crate) name: String,
    pub(crate) project: String,
    pub(crate) project_code: String,
    pub(crate) client: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Work {
    pub(crate) start_datetime: Option<DateTime<Utc>>,
    pub(crate) end_datetime: Option<DateTime<Utc>>,
    pub(crate) notes: Option<String>,
    pub(crate) first_name: String,
    pub(crate) last_name: String,
    pub(crate) task: Task,
}

impl Work {
    pub(crate) fn new(first_name: String, last_name: String, task: Task) -> Self {
        Self {
            start_datetime: None,
            end_datetime: None,
            notes: None,
            first_name,
            last_name,
            task,
        }
    }

    pub(crate) fn hours(&self) -> Option<String> {
        let end_datetime = self.end_datetime.as_ref()?;
        let start_datetime = self.start_datetime.as_ref()?;
        let duration = end_datetime.clone().signed_duration_since(start_datetime);
        let minutes = duration.num_minutes();
        let hours = minutes as f64 / 60.0;
        Some(format!("{hours:.2}"))
    }

    pub(crate) fn date_string(&self) -> Option<String> {
        self.start_datetime
            .as_ref()
            .map(|dt| dt.date_naive().to_string())
    }

    pub(crate) fn starts_within(
        &self,
        start_date: &Option<DateTime<Utc>>,
        end_date: &Option<DateTime<Utc>>,
    ) -> bool {
        match (self.start_datetime, start_date, end_date) {
            (_, None, None) => true,
            (Some(wsd), Some(csd), None) => wsd >= *csd,
            (Some(wsd), None, Some(ced)) => wsd <= *ced,
            (Some(wsd), Some(csd), Some(ced)) => wsd >= *csd && wsd <= *ced,
            _ => false,
        }
    }
}
