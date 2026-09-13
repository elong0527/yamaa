# Parse the site from the identifier with a collected fallback

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-identifier-parsing.html)

**Goal:** derive the parsed site, the site to use, and the display
reference (`SITEIDP`, `SITEID`, and `SUBJREF`) for each subject, carrying
the collected identifiers through.

**Input:** demographics with the study identifier, the unique subject
identifier, the collected subject number, and the collected site
(`SITEID`).

**Variables:**

- `SITEIDP` is the site code read from the middle of the unique subject
  identifier when it has the study, site, and exactly four-digit
  subject-number form; empty otherwise.
- `SITEID` is the site to use: the parsed value when present, else the
  collected site, else `UNKNOWN`.
- `SUBJREF` is the display reference combining the site to use and the
  collected subject number with a colon; `UNKNOWN` when the subject
  number is absent.

**Note:** keeping both the parsed and the final site shows which subjects
fell back to the collected value.

**Standard:** ADaM | **Domain:** ADSL
