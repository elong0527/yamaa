# Reject a listing stored under an unnamed format

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-source-unknown-profile.html)

**Goal:** build a subject listing carrying `SITEID`.

**Input:** collected subject listing stored as `input/dm.txt`,
holding `SITEID`.

**Variables:**

- `SITEID` would be the site the subject enrolled at, taken
  directly from the collected listing.

The file name says nothing about how to read it. Its bytes happen
to use commas today, but nothing states that, and a reader that
decided by looking inside could read the same file differently
another time or differently from the next reader. The run is
rejected before any data is read, so no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Store the listing under the name of the format it is in, so that its reader
is chosen by what the study declares rather than by inspection:

```yaml
datasets:
  DM:
    path: input/dm.csv
```

A listing kept in some other format is converted before the study reads it,
and the converted file carries the name of what it now holds.
