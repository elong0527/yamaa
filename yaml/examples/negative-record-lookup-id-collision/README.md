# Reject a first treatment named after its own source

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-record-lookup-id-collision.html)

**Goal:** carry the first treatment received in `TRT01A`, one
record per subject.

**Input:** demographics records plus exposure records carrying the
administration sequence number (`EXSEQ`), the treatment name
(`EXTRT`), and the administration start date (`EXSTDTC`).

**Variables:**

- `TRT01A` would contain the treatment name from the earliest
  exposure record.

The chosen-record name `EX` already names the exposure records, so
a read of `EX.EXTRT` cannot say whether it means the treatment of
the chosen record or of every exposure record. The two readings
differ whenever a subject has more than a single administration, so
the run is rejected before any data is read and no artifact is
accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Give the record lookup a name that is distinct from every dataset and from
the output domain, then read through that name:

```yaml
record_lookups:
  - id: FIRSTEX
    dataset: EX
    order_by: [EX.EXSTDTC, EX.EXSEQ]
    keep: first

# ...
derivation:
  source: FIRSTEX.EXTRT
```
