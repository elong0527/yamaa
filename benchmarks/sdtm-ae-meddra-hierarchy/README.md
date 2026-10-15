# MedDRA Hierarchy from Coder-Assigned LLT Codes

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ae-meddra-hierarchy.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive one AE record per collected event, retaining the reported
term and carrying the full coded MedDRA hierarchy from the LLT code assigned
by a medical coder.

**Input:** event records with `AETERM` and a coder-assigned `AELLTCD`, plus a
small MedDRA-shaped extract with one row per LLT-to-SOC path. Its LLT, PT,
HLT, and HLGT names and all codes are invented; this is not an official
MedDRA release.

**Variables:**

- `AETERM` is the event text as reported, even when it differs from the
  selected lowest-level term.
- `AELLTCD` is the coder-assigned lowest-level term code; `AELLT` is its
  dictionary name.
- `AEPTCD` and `AEDECOD` are the preferred-term code and name.
- `AEHLTCD` and `AEHLT` are the high-level term code and name.
- `AEHLGTCD` and `AEHLGT` are the high-level group term code and name.
- `AEBODSCD` and `AEBODSYS` are the primary SOC code and name.
- `AESOCCD` and `AESOC` repeat that primary SOC code and name.

**Note:** the first two coded LLTs share a PT with primary and secondary SOC
paths. The paths also have different HLT and HLGT values, so every hierarchy
field must come from the primary path. Each secondary row appears first in
the extract. An unknown or absent LLT code leaves the derived hierarchy
blank; an unknown assigned code remains visible in `AELLTCD` for review.

**Standard:** SDTM | **Domain:** AE
