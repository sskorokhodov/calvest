# calvest - iCalendar to CSV Converter for Harvest

## Overview

This tool is for those, who uses [Harvest][harvest] but prefers tracking my
tasks in a calendar app.

The process of manually transferring tasks from the calendar to Harvest is
time-consuming, erorr prone, and tedeous. **calvest** is a command-line tool
designed to transform data in iCalendar (`.ics`) format into a structured CSV
file that can be imported into Harvest.

At the moment **calvest** is only tested with the iCal data exported from Google
Calendar.

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

## Installation

Ensure you have **Rust** installed on your machine. If Rust is not installed,
you can install it by following the instructions on the [official Rust
website](https://www.rust-lang.org/tools/install).

1. **Clone the repository and install the binary**

   Clone the repository and install the binary into your Cargo installation root
   `bin` folder.

   ```bash
   git clone https://github.com/yourusername/calvest.git
   cd calvest
   cargo install --path .
   ```

   Ensure your [Cargo installation root `bin` folder][cargo-install] is on the
   `"${PATH}"`.

2. **Verify the Installation**

   Confirm that `calvest` is installed correctly by checking its version:

   ```bash
   calvest --version
   ```

2. **Optionally, print completions for you shell**

   Print completions for your shell to the corresponding file, e.g.,

   ```bash
   calvest --print-completions zsh > ~/.oh-my-zsh/cache/completions/_calvest
   ```

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

The console output may look like below

```text
Collecting events from 2025-01-01 (inclusive) to 2025-02-01 (exclusive) ...

Events collected. Events total: 95
```

[harvest]: https://www.getharvest.com/
[cargo-install]: https://doc.rust-lang.org/cargo/commands/cargo-install.html
