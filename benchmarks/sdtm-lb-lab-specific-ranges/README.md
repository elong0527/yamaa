# Lab-Specific Reference Ranges

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-lb-lab-specific-ranges.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** pick the reference range the way a multi-lab study does -- by
lab, test, sex and age band, and by the period the range was in effect --
and flag each result: `LBSTNRLO`, `LBSTNRHI`, and `LBNRIND`.

**Input:** lab results as Operational Data Model (ODM) item data -- one row
per collected item, with the test code, the collecting lab, the collection
date and the numeric result as `IT.LB.*` items of a repeating `IG.LB` panel,
and recorded sex and birth date as `IT.DM.*` items -- plus a reference
dictionary keyed by lab, test code, sex and age band, each entry carrying the
dates it was in effect, the unit and the lower and upper limits.

**Variables:**

- `LBTESTCD` is the test code from the `IT.LB.LBTESTCD` item.
- `SEX` is recorded sex from the `IT.DM.SEX` item.
- `LBNAM` is the collecting lab from the `IT.LB.LBNAM` item; different labs
  may carry different ranges for the same test.
- `LBSTRESN` is the numeric result in standard units from the
  `IT.LB.LBSTRESN` item; missing when the result was not collected.
- `LBORRESU` is the unit of the reference entry for the result's lab, test
  code and sex whose age band holds the subject's age at collection and
  which was in effect on the collection date; blank, like both limits, when
  no entry matches.
- `LBSTNRLO` is the lower reference limit in standard units from that
  entry.
- `LBSTNRHI` is the upper reference limit in standard units from that
  entry.
- `LBNRIND` is `LOW` when the result is below the lower limit, `HIGH`
  when it is above the upper limit, and `NORMAL` otherwise, including a
  result with no matching reference entry; blank when the result itself is
  missing.

**Note:** when a lab revises a range, the entry in effect on the collection
date wins -- a result collected on the first day of the new range is judged
against the new limits -- and a subject whose age falls on an age-band
boundary takes the band that starts at that age. Each output row is built
from one `IT.LB.LBDTC` item, with the other items of its panel read
alongside; `LBSEQ` is the panel's repeat key.

**Standard:** SDTM | **Domain:** LB
