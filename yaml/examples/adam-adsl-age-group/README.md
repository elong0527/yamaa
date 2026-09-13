# Group subjects by pooled age group

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-age-group.html)

**Goal:** carry each subject's collected age and derive `AGEGR1`
and `AGEGR1N` to pool subjects into age groups.

**Input:** demographics (DM) records carrying age (`AGE`) and age
units (`AGEU`).

**Variables:**

- `AGEGR1` is the pooled group: `<18` when age is below 18, `18-64`
  when age is 18 through 64 inclusive, and `>64` when age is above
  64; `Missing` when `AGE` is missing.
- `AGEGR1N` is the numeric rank of `AGEGR1`: `1` for `<18`, `2` for
  `18-64`, and `3` for `>64`; empty when `AGE` is missing.

**Note:** both flags share the same bands, so the number always
matches the text for each subject.

**Standard:** ADaM | **Domain:** ADSL
