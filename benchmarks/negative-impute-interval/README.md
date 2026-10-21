# Reject an Onset Recorded as a Period

**Goal:** derive each adverse event's analysis start date (`ASTDT`) from
the collected start, completing a partial start to the earliest date it
allows.

**Input:** adverse event records, each identified by study, subject and
sequence, carrying the collected start (`AESTDTC`) as ISO 8601 text: a
full date, a date whose month is unknown written with a hyphen in its
place (`2025---15`), or a period between two dates
(`2025-03-01/2025-03-10`).

**Variables:**

- `ASTDT` would be the start date: a full date stands, and a start whose
  month is unknown is completed from its year alone, to 1 January,
  whatever day was collected. A start recorded as a period between two
  dates names no single date to complete, so the run is rejected and no
  artifact is accepted.

## How to fix

Decide clinically which date a period of uncertainty stands for. For the
earliest possible onset, take the period's first date into its own column
before completing it:

```yaml
- name: ASTDTC
  type: str
  derivation:
    str_extract:
      source: AE.AESTDTC
      pattern: "^[^/]+"
```

and complete `ASTDTC` instead of `AE.AESTDTC`. If an onset recorded as a
period should have no analysis date, declare that outcome with
`invalid: {literal: null}` on `impute`; it then answers every start that
names no single date.
