# Lab BDS Build

**Goal:** build a laboratory (LB) basic data structure (BDS)
analysis dataset for alanine aminotransferase (ALT) and
aspartate aminotransferase (AST): flag each subject's baseline
record (`ABLFL`), carry its value (`BASE`), compute change
(`CHG`) and percent change (`PCHG`), and add one ALT record
converted to International System of Units (SI) (`ALTSI`),
alongside each subject's treatment start date (`TRTSDT`) and
actual treatment (`TRT01A`).

**Input:** collected ALT and AST results with collection dates,
numeric results, and units, plus subject-level treatment start
dates and actual treatments.

**Variables:**

- `ADT` is the collection date of the result.
- `TRTSDT` is the subject's treatment start date.
- `TRT01A` is the subject's actual treatment.
- `PARAM` names the parameter with its unit: `U/L` for the
  collected results, and `ukat/L` on the SI record.
- `AVAL` is the collected numeric result, or the result times
  0.0167 on the SI record.
- `ABLFL` is `Y` on the latest record on or before the
  treatment start date for each subject and parameter; blank
  otherwise.
- `BASE` is the `AVAL` of the flagged baseline record,
  repeated on every record for the same subject and parameter.
- `CHG` is `AVAL` minus `BASE` on a record collected after the
  treatment start date; empty on the baseline record.
- `PCHG` is 100 times `CHG` divided by `BASE`; empty wherever
  `CHG` is, and when the baseline value is zero, since the
  percentage is undefined.

**Note:** a collected result with no numeric value produces no
record. A subject with no collected results has no records: the
treatment dates enrich records that already exist and never add
one. A subject with a single collected result still gets a
baseline flag, and a result collected on the treatment start
date itself counts as baseline.
