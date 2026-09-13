# Normalize country and group it into a region

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-geography-normalization.html)

**Goal:** give each subject a normalized country in `COUNTRY`
and its region in `REGION1`.

**Input:** demographics (`DM`) rows carrying the collected country
(`COUNTRY`) as reported, in mixed case or empty.

**Variables:**

- `COUNTRY` is the collected country from demographics in upper
  case, so the same country reported in different cases becomes
  one value; a subject with no collected country is `UNKNOWN`.
- `REGION1` is the region for that country: `USA` and `CAN` give
  `North America`, `DEU` gives `Europe`, and any other value,
  including `UNKNOWN`, gives `Rest of World`, so every subject
  has a region.

**Note:** case differences never split a country, and the fallback
values mean every subject has both a country and a region.

**Standard:** ADaM | **Domain:** ADSL
