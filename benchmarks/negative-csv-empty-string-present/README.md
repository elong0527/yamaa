# Reject Empty Text Kept as a Value in a Comma-Separated File

**Goal:** carry the collected free-text race (`RACEOTH`) onto the
demographics domain, keeping an empty entry as collected text.

**Input:** collected demographics in a comma-separated file, one record per
subject.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `RACEOTH` would be the free-text race as collected.

**Note:** the input is declared to keep an empty text entry as a collected
value, distinct from a value that was never collected. A comma-separated
file writes both as an empty field, so it cannot carry that distinction,
and the run is rejected before any data is read; no artifact is accepted.

## How to fix

Treat an empty field as not collected, which is what a comma-separated file
can express, by removing `empty_string: present` (the default is
`missing`):

```yaml
input:
  RAW: input/dm_raw.csv
```

When the study does need to tell collected empty text from a value never
collected, export the source as Parquet, which stores the two differently,
and keep `empty_string: present` on that input.
