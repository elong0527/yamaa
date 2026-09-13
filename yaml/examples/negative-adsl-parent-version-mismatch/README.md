# Reject shared definitions from another language version

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adsl-parent-version-mismatch.html)

**Goal:** prepare subject records from definitions shared
through a reusable file.

**Input:** demographics (DM) records identified by subject.

**Variables:**

No variables are requested, so no output is produced. The entry
file reuses `layers/parent.yaml`, which declares language version
`2.0` while the entry file declares `1.0`, so the two files cannot
be combined into one set of definitions, and the run is rejected
before any data is read.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Migrate the parent file and the entry file together, then give every layer the
same `schema_version` as the active bundle.
