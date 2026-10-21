# Carry Subject-Level Variables onto Adverse Events by Name

**Goal:** give every adverse event record its subject's age (`AGE`), sex
(`SEX`), planned arm (`ARM`) and date of first exposure to treatment
(`TRTSDT`) from the subject-level analysis dataset (ADSL), each under the
name, type and label ADSL gives it.

**Input:** two spec files, run in order:

- `spec_adsl.yaml` builds ADSL from collected demographics, one record per
  subject with the first exposure date as text, and saves it as `adsl.csv`;
- `spec_adae.yaml` is the entry: it reads adverse event records carrying the
  reported term (`AETERM`) and the saved `adsl.csv`, and names the four
  variables to take from it without declaring them again.

**Variables:**

- `AGE`, `SEX` and `ARM` are the subject's age in years, sex and planned arm,
  repeated on each of the subject's events;
- `TRTSDT` is the date of the subject's first exposure to treatment, still a
  date on the events; it stays empty for a subject never exposed, such as a
  screen failure.

**Note:** a variable taken by name from ADSL keeps everything the spec that
builds ADSL declares for it, so the two datasets cannot disagree on its type
or label. Each event finds its subject's one ADSL record by the study and
subject identifiers; a subject without adverse events adds no event record.
