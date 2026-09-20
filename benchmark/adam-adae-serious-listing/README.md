# Serious Event Listing

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-serious-listing.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** list serious adverse events.

**Input:** collected adverse event (AE) records, including serious and
non-serious events, each with a reported term (`AETERM`) and a serious
flag (`AESER`).

**Variables:**

- `AESER` marks the event as serious; it holds `Y` on every listed
  record, since a non-serious event is not listed.

**Note:** when no event is serious, the listing is empty but still
carries its variable names, so an empty listing is distinct from one
never produced.

**Standard:** ADaM | **Domain:** ADAE
