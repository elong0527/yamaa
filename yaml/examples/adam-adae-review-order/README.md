# Order a subject's events for medical review

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-review-order.html)

**Goal:** arrange each subject's (`USUBJID`) adverse event (AE)
records so the reviewer reads the worst-severity events first, and
within one severity the earliest onset first.

**Input:** adverse event records carrying sequence number (`AESEQ`),
reported term (`AETERM`), onset date (`AESTDTC`), and severity
(`AESEV`).

**Variables:**

- `SEVORD`: numeric rank of severity (`ASEV`), `1` for MILD, `2`
  for MODERATE, `3` for SEVERE; blank when no severity was
  reported. It sets the worst-first order and is not kept in the
  result.

**Note:** the result keeps the reported term (`AETERM`), onset date
(`ASTDT`), and severity (`ASEV`) for every event: `ASTDT` carries
the collected onset (`AESTDTC`), `ASEV` carries the reported
severity (`AESEV`), and `ASEQ` repeats the collection sequence
(`AESEQ`). An event with no onset date precedes the dated events of
its severity, so the record that still needs a date is read rather
than overlooked. An event with no reported severity follows every
reported severity, because an absent severity is not a mild one.
Two events a reviewer cannot tell apart -- the same severity on the
same date -- stay in collection (`AESEQ`) order.

**Standard:** ADaM | **Domain:** ADAE
