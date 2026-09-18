# Reject shared definitions from a remote location

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-adsl-remote-parent.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** reviewed - has discussion comments or GitHub issues.

**Goal:** attempt to prepare subject records reusing shared
definitions named by `https://example.test/organization.yaml`.

**Input:** demographics records carrying the unique subject
identifier.

**Variables:**

The requested result names no variables. It would carry subject
records prepared from the shared definitions, but no row is
produced because the run is rejected before any data is read: a
remote resource can change independently and cannot provide a
reproducible local build.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Review and store the parent file locally, then reference it with a relative or
absolute filesystem `parents` path. Do not use a URL or URI.
