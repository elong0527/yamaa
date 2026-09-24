# Arm Assignment

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-arms.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** assign each subject a planned arm (`ARMCD`, `ARM`) and
an actual arm (`ACTARMCD`, `ACTARM`), with a reason (`ARMNRS`)
when an arm is blank and a free-text fallback (`ACTARMUD`) for a
treatment that matches no planned arm.

**Input:** one randomization record per subject carrying the planned
arm code and description plus a screen-failure flag, and exposure
records carrying the treatment received.

**Variables:**

- `ARMCD` / `ARM` are the planned arm code and description from
  randomization; blank when the subject was never randomized.
- `ACTARMCD` / `ACTARM` are the actual arm code and description for
  the treatment received; blank when the subject was never treated or
  the treatment matches no planned arm.
- `ARMNRS` is the reason an arm is blank. A subject with neither a
  planned nor an actual arm gets `SCREEN FAILURE` when flagged as a
  screen failure and `NOT ASSIGNED` otherwise; a randomized subject
  never treated gets `NOT TREATED`. It is blank otherwise, including
  for a treatment that matches no planned arm, which `ACTARMUD`
  records instead.
- `ACTARMUD` is the treatment received as collected, kept only when it
  matches no planned arm; blank otherwise.

**Note:** the planned arm follows randomization while the actual arm
follows exposure, so a subject treated with the other planned arm
carries different codes in each.

**Standard:** SDTM | **Domain:** DM
