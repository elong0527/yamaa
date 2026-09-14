# Carry the MedDRA hierarchy from the coder's term code

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-ae-meddra-hierarchy.html)

**Goal:** fill every hierarchy level for each collected adverse
event from the lowest-level term code the coder chose: the term
(`AELLT`, `AELLTCD`), the preferred term (`AEDECOD`, `AEPTCD`),
the high level term (`AEHLT`, `AEHLTCD`), the high level group
term (`AEHLGT`, `AEHLGTCD`), and the body system (`AEBODSYS`,
`AEBODSCD`).

**Input:** collected adverse-event rows carrying the coder's
lowest-level term code, plus a dictionary extract carrying each
code's full path from lowest-level term through body system.

**Variables:**

- **AELLT**: lowest-level term whose code equals the coder's
  code; `NOT CODED` when no code equals it, including a blank
  code.
- **AELLTCD**: code as the coder recorded it, kept exactly as
  written; blank when none was recorded.
- **AEDECOD**: preferred term on the code's path; `NOT CODED`
  when the code has no path.
- **AEPTCD**: preferred term code on the code's path; `NOT
  CODED` when the code has no path.
- **AEHLT**: high level term on the code's path; `NOT CODED`
  when the code has no path.
- **AEHLTCD**: high level term code on the code's path; `NOT
  CODED` when the code has no path.
- **AEHLGT**: high level group term on the code's path; `NOT
  CODED` when the code has no path.
- **AEHLGTCD**: high level group term code on the code's path;
  `NOT CODED` when the code has no path.
- **AEBODSYS**: body system on the code's path, following the
  primary path when the preferred term sits under two body
  systems; `NOT CODED` when the code has no path.
- **AEBODSCD**: body system code on the code's path, on the
  same primary path as **AEBODSYS**; `NOT CODED` when the code
  has no path.

**Note:** the coder picks a code rather than matching text, so
an event whose collected term matches no dictionary entry still
codes when the coder assigned one. The dictionary extract here
is invented and carries no release version; a preferred term
with a primary and a secondary body system keeps both
alongside, and the hierarchy follows the primary one.

**Standard:** SDTM | **Domain:** AE
