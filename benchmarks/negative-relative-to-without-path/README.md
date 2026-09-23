# Reject Detached Path Anchor

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-relative-to-without-path.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the subject identifier from demographics into
subject-level records, reading the demographics file the study keeps
beside its own entry file.

**Input:** demographics holding the subject identifier, stored beside
the entry file.

**Variables:**

The parent file names the demographics file relative to its own
directory. The entry file asks for that inherited location to be read
from the entry file's directory instead, but writes no location of its
own. A request to read a location from the entry file's directory
applies only to a location written beside it in the same file, so here
it applies to nothing. The run is rejected before any data is read and
no record is produced.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide which file owns the location of the study's demographics. If the
shared parent file should keep declaring it, write `relative_to: entry`
beside the path in that parent file:

```yaml
input:
  DM:
    path: input/dm.csv
    relative_to: entry
```

If only this study reads it from beside its own entry file, restate the
path in the entry file and drop `relative_to`, since a path the entry
file writes is already read from its directory:

```yaml
input:
  DM: input/dm.csv
```
