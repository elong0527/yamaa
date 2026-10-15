# Normalize the Collected Country and Assign Its Region

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-country-region.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** give each subject a normalized country in `COUNTRY`
and its region in `REGION1`.

**Input:** demographics (`DM`) rows carrying the collected country
(`COUNTRY`) as reported, in mixed case or empty, including a country
outside the three listed below.

**Variables:**

- `COUNTRY` is the collected country from demographics in upper
  case; a subject with no collected country is `UNKNOWN`. Every
  collected country passes through: there is no allowed-countries
  check, so the run never stops on a new country.
- `REGION1` is the region for that country: `USA` and `CAN` give
  `North America`, `DEU` gives `Europe`, and any other country
  (including `UNKNOWN`) gives `Rest of World`.

**Note:** case differences never split a country (`usa` and `USA`
are one), and the fallback values mean every subject has both a
country and a region.

**Standard:** ADaM | **Domain:** ADSL
