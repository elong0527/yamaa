# Reject a site identifier taken from an uncaptured part

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-str-extract-undeclared-group.html)

**Goal:** read `SITEID` out of the subject identifier for each
subject record.

**Input:** collected demographics with one record for each subject,
carrying the subject identifier.

**Variables:**

- `SITEID` would hold the site portion taken from the second part
  of the subject identifier matching
  `^CATH-([^-]+)-[0-9]{4}$`, and would be blank when the subject
  identifier is blank or does not fit that form; but no row is
  produced because the match sets aside only one part, so the
  request is rejected before any data is read and no artifact is
  accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide which part of the subject identifier names the site, then take the
part the rule actually sets aside. Parts are numbered from 1 in the order
their opening parentheses appear, so the site here is part 1:

```yaml
str_extract:
  source: USUBJID
  pattern: '^CATH-([^-]+)-[0-9]{4}$'
  group: 1
```

Set aside a second part when the correction genuinely needs one, and number
it by that same position. Here part 2 is the four-digit subject number:

```yaml
str_extract:
  source: USUBJID
  pattern: '^CATH-([^-]+)-([0-9]{4})$'
  group: 2
```
