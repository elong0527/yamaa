# Reject shared definitions from a remote location

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adsl-remote-parent.html)

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
