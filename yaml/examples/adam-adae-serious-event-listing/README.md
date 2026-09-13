# Serious event listing

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-serious-event-listing.html)

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
