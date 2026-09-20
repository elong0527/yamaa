# Reject Cyclic Parent

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-cyclic-parent.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** prepare subject records from definitions shared
through a reusable file.

**Input:** demographics (DM) records identified by subject.

**Variables:**

No variables are requested, so no output is produced. The entry
file reuses `layers/parent.yaml`, which points back to the entry
file, so no stable set of definitions can be reached, and the run
is rejected before any data is read.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Remove the backward `parents` reference so that every path
through the chain terminates. Keep genuinely shared definitions
in one common ancestor.
