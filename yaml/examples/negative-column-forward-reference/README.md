# Reject a doubled value built from a later column

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-column-forward-reference.html)

**Goal:** carry the collected analysis value in `AVAL` and add
its doubled value in `AVALDOUBLED`, one record per collected
result.

**Input:** collected laboratory (LB) results carrying the
collected result in `AVAL`, identified by study and subject.

**Variables:**

- `AVAL` would contain the collected result, taken from `AVAL`
  in the laboratory input; missing when the collected result is
  missing.
- `AVALDOUBLED` would contain `AVAL` multiplied by 2; missing
  when the collected result is missing.

The doubled value is listed before the collected value it reads,
so it has nothing to read when its turn comes. The run is
rejected before any data is read, and no artifact is accepted, so
no row is produced.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

List `AVAL` before `AVALDOUBLED`, so every value reads only values already
computed.
