# Code reported terms against a medical dictionary

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-ae-dictionary-coding.html)

**Goal:** derive one record per collected adverse event, keeping
the reported term (`AETERM`) and adding the coded preferred term
(`AEDECOD`) and body system (`AEBODSYS`).

**Input:** collected adverse-event rows carrying the reported term,
plus a Medical Dictionary for Regulatory Activities (MedDRA)
extract carrying the lowest-level, preferred, and body-system
names.

**Variables:**

- `AETERM` is the term as reported, kept exactly as written; blank
  when no term was reported.
- `AEDECOD` is the preferred term for the reported term, taken as
  the preferred name whose lowest-level name exactly equals the
  reported term, including letter case; `NOT CODED` when no
  lowest-level term equals it, including a blank reported term.
- `AEBODSYS` is the body system for the reported term, taken as
  the body-system name whose lowest-level name exactly equals the
  reported term, including letter case; `NOT CODED` when no
  lowest-level term equals it, including a blank reported term.

**Note:** the coded terms follow the recorded dictionary, MedDRA
version `26.1`, since the same reported term can code differently
between releases.

**Standard:** SDTM | **Domain:** AE
