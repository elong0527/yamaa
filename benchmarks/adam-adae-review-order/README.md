# Review Ordering

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-review-order.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build a reviewer-ordered adverse event listing carrying
`ASEQ`, `AETERM`, `ASTDT`, and `ASEV` for every event, worst severity
first.

**Input:** adverse event records carrying sequence number (`AESEQ`),
reported term (`AETERM`), onset date (`AESTDTC`), and severity
(`AESEV`).

**Variables:**

- `AETERM`: the adverse event term exactly as reported.
- `ASTDT`: the collected onset date (`AESTDTC`); blank when the event
  has no recorded onset date.
- `ASEV`: the reported severity (`AESEV`); blank when no severity was
  reported.

**Note:** the reviewer reads the worst-severity events first, and
within one severity the earliest onset first. An event with no onset
date comes before the dated events of its severity, so a record still
missing its date is read rather than overlooked. An event with no
reported severity comes after every reported severity, because an
absent severity is not a mild one. Events the reviewer cannot tell
apart -- the same severity on the same date -- stay in collection
(`AESEQ`) order.

**Standard:** ADaM | **Domain:** ADAE
