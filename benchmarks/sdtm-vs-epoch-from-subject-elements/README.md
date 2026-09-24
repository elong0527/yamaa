# Epoch from Subject Elements

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-vs-epoch-from-subject-elements.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each collected vital-signs result with its test, result,
and collection date-time, and add the trial period from the subject's
own elements: `VSTESTCD`, `VSORRES`, `VSDTC`, and `EPOCH`.

**Input:** collected vital-signs rows with test, result, and collection
date-time; and the subject's elements with the start and end of each
element.

**Variables:**

- `VSDTC` is the collection date-time, carried over unchanged; missing
  when no date was collected.
- `EPOCH` is the trial period of the element in progress at the
  collection date-time: the element whose start is on or before the
  collection and whose end is on or after it. It is missing when the
  reading has no date or only a partial one, or falls outside every
  element, such as before the first element starts.

**Note:** the first-dose day shows why a date-time matters: a pre-dose
reading on that day belongs to screening and a post-dose reading to
treatment. A date-only collection reads as the start of that day, so it
falls in the element in progress at day-start; a reading exactly at an
element's start belongs to that element.

**Standard:** SDTM | **Domain:** VS
