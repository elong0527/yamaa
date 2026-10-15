# Code Reported Terms Against a Medical Dictionary

**Goal:** derive one record per collected adverse event, keeping
the reported term (`AETERM`) and adding the coded preferred term
(`AEDECOD`) and body system (`AEBODSYS`).

**Input:** collected adverse-event rows carrying the reported term,
plus a Medical Dictionary for Regulatory Activities (MedDRA)
extract carrying the lowest-level, preferred, and body-system
names.

**Variables:**

- `AEDECOD` is the preferred term of the dictionary entry whose
  lowest-level term matches the reported term.
- `AEBODSYS` is the body system of that same entry.

**Note:** a reported term codes only when it equals a lowest-level
term exactly, including letter case; the preferred term comes from
the matched dictionary entry, so it can differ from the reported
term. A term with no exact match, or a blank term, gives `NOT
CODED` in both coded variables. `NOT CODED` is not a MedDRA term:
it marks an event whose coding must be resolved before delivery.
The coded terms follow the recorded dictionary, MedDRA version
`26.1`, since the same reported term can code differently between
releases.
