# Reject a Vital Signs Date-Time That Would Invent Seconds

**Goal:** carry each vital signs result with its collection date and time
(`VSDTC`).

**Input:** collected vital signs records, each identified by study,
subject and sequence, carrying the test, the result and the collection
date and time as ISO 8601 text, collected to the minute or the second.

**Variables:**

- `VSDTC` would be the collection date and time. Held as a date-time
  value, a time collected to the minute, `2026-01-06T08:30`, would be
  written `2026-01-06T08:30:00`, claiming seconds no one collected. A
  tabulation dataset keeps the collected text, so the run is rejected
  before any data is read and no artifact is accepted.

## How to fix

Keep the date and time as the collected text, at the precision it was
collected, and state its ISO 8601 type for reviewers. Text collected to
the minute or a coarser precision is a partial date-time:

```yaml
- name: VSDTC
  type: str
  label: Date/Time of Measurements
  derivation: VS_RAW.VSDTC
  submission:
    core: Exp
    data_type: partialDatetime
    origins:
      - type: Collected
        source: Investigator
```

A comparison or lookup that needs a moment completes the text into an
internal `datetime` column that the dataset does not carry.
