# Calvest - iCalendar to CSV Converter for Harvest

## Overview

Harvest is a time-tracking and invoicing software. **calvest** is a command-line
tool designed to transform data in iCalendar (iCal) format into a structured CSV
file that can be imported into Harvest.

## Features

- **iCal Event Parsing**: Calvest reads iCalendar files and extracts event
  details including start and end times, summaries, and any additional specified
  properties.
- **Configurable Task Mapping**: It allows users to define rules for mapping
  events to specific tasks, projects, and clients based on regular expression
  patterns found in event summaries.
- **Default Task Settings**: Users can specify default tasks, projects, and
  clients ensuring that all events are appropriately categorized when no
  specific pattern matches are found.
- **Date Range Filtering**: Provides options to filter events based on custom
  date ranges or predefined periods like "last month" and "this month."
- **CSV Output**: The tool outputs a CSV file that includes event details
  alongside user-defined extra properties, ready for direct import into Harvest.

## Example

It can be convenient to create a Bash/Just script like this ...

```bash
#!/bin/bash

# calvest.sh

input='./my_calendar.ics'

default_project='Default Project'
default_task='Default Task'
default_client='Default Client'
first_name='First-name'
last_name='Last-name'

output='./'"${first_name}""${last_name}"'_harvest_'"$( date -I )"'.csv'

calvest \
  --input="${input}" \
  --output="${output}" \
  --first-name="${first_name}" \
  --last-name="${last_name}" \
  --default-task "${default_task}" "${default_project}" "${default_client}" \
  --task 'Daily' 'My Project' 'My Client' '^My Client *:: *Daily *$' \
  --task 'My Task 1' 'My Project 1' 'My Client 1' '^My Client 1 :: *' \
  ${@}
```

... and then run it, for example, like this

```bash
calvest.sh --timeframe='last-month'
```

The output looks as follows

```text
Collecting events from 2025-01-01 (inclusive) to 2025-02-01 (exclusive) ...

Events collected. Events total: 95
```
