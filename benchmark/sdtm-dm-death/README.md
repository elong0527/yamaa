# Derive death date and flag from disposition and adverse events

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-dm-death.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** reviewed - has discussion comments or GitHub issues.

**Goal:** derive date/time of death (`DTHDTC`) and death flag (`DTHFL`) for
each demographics subject.

**Input:** demographics subjects, disposition records with status and start
date, and adverse events with outcome and end date.

**Variables:**

- `DTHDTC` is the disposition death date, or the fatal adverse event end date
  when disposition has no death; empty when neither source records a death.
- `DTHFL` is `Y` when either source supplies a death date; empty otherwise.

**Note:** when the sources record different death dates, the disposition date
wins.

**Standard:** SDTM | **Domain:** DM
