# Arm Assignment

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-arms.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** assign each subject a planned arm (**ARMCD**, **ARM**) and
an actual arm (**ACTARMCD**, **ACTARM**), with a reason (**ARMNRS**)
when an arm is blank and a free-text fallback (**ACTARMUD**) for a
treatment that matches no planned arm.

**Input:** one randomization record per subject carrying the planned
arm code and description plus a screen-failure flag, and exposure
records carrying the treatment received.

**Variables:**

- **ARMCD**: planned arm code from randomization; blank when the
  subject was never randomized.
- **ARM**: planned arm description from randomization; blank when the
  subject was never randomized.
- **ACTARMCD**: actual arm code for the treatment received; blank when
  the subject was never treated or the treatment matches no planned
  arm.
- **ACTARM**: actual arm description for the treatment received; blank
  under the same conditions as **ACTARMCD**.
- **ARMNRS**: reason a planned or actual arm is blank: `SCREEN FAILURE`
  for a screen failure, `NOT ASSIGNED` for a subject entered but never
  randomized, `NOT TREATED` for a randomized subject never treated;
  blank otherwise.
- **ACTARMUD**: treatment received as collected, kept only when it
  matches no planned arm; blank otherwise.

**Note:** the planned arm follows randomization while the actual arm
follows exposure, so a subject treated with the other planned arm
carries different codes in each; only a treatment matching no planned
arm leaves the actual arm blank with its description kept separately.

**Standard:** SDTM | **Domain:** DM
