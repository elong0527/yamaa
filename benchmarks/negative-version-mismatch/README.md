# Reject Version Mismatch

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-version-mismatch.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** prepare subject records from definitions shared
through a reusable file.

**Input:** demographics (DM) records identified by subject.

**Variables:**

No variables are requested: the entry file names no output
variables.

**Note:** the entry file reuses `layers/parent.yaml`, which declares
language version `2.0` while the entry file declares `1.0`. Files
written for different versions cannot be combined into one set of
definitions, so the run is rejected before any data is read and no
artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Migrate the parent file and the entry file together, then give every layer the
same `schema_version` as the active bundle. The bundle in this repository is
version `1.0`, so here the parent's `"2.0"` is the value to correct; raising
both files to `"2.0"` is rejected the same way.
