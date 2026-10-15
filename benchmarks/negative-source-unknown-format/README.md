# Reject Unknown Format

**Goal:** build a subject listing carrying `SITEID`.

**Input:** collected subject listing stored as `input/dm.txt`,
holding `SITEID`.

**Variables:**

- `SITEID` would be the site the subject enrolled at, taken
  directly from the collected listing.

The file's `.txt` ending names neither of the two formats a listing
can be read from, `.csv` (comma-separated text) or `.parquet`, so
nothing says how to read it. Its bytes happen to use commas today,
but a reader that decided by looking inside could read the same
file differently another time or differently from the next reader.
The run is rejected before any data is read, so no artifact is
accepted.

## How to fix

Store the listing under the name of the format it is in, so that its reader
is chosen by what the study declares rather than by inspection:

```yaml
input:
  DM:
    path: input/dm.csv
```

A listing kept in some other format is converted before the study reads it,
and the converted file carries the name of what it now holds.
