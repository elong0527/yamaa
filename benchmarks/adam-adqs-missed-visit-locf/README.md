# Carry the last post-baseline score to each missed scheduled visit

**Goal:** give every subject in the efficacy population a total score at
each scheduled visit, adding a last observation carried forward (LOCF)
record for each visit missed, with the analysis value (`AVAL`), the
record-type flag (`DTYPE`), the collected record it comes from (`QSSEQ`),
and the efficacy population flag (`EFFFL`).

**Input:** one subject-level record per subject carrying the efficacy
population flag, and the SDTM questionnaire (QS) records of a total
score, here the ADAS-Cog total (`ACTOT`), one per subject and visit,
including records whose score was not done. Each record's visit gives
the analysis visit (`AVISIT`, numbered `AVISITN`).

**Variables:**

- `AVAL`: the collected score on a collected record, and empty when the
  score was not collected. On a carried-forward record, the score from
  the closest earlier post-baseline visit that has one for the same
  subject.
- `DTYPE`: empty on a collected record; `LOCF` on a record added for a
  missed scheduled visit.
- `QSSEQ`: the collected record's own sequence number, and on a
  carried-forward record the sequence number of the record whose score
  it carries.
- `EFFFL`: the subject's efficacy population flag, repeated on every
  record.

**Note:** the protocol schedules Weeks 8, 16, and 24 for every subject in
the efficacy population. A scheduled visit is missed when the subject has
no collected score there; a collected record whose score is empty stays,
and the carried score goes on its own record beside it. Only post-baseline
scores are carried, since baseline is the reference a change is measured
from, so a missed visit with no earlier post-baseline score gains no
record. Subjects outside the efficacy population keep their collected
records and gain none. The input carries at most one record per subject
per visit.
