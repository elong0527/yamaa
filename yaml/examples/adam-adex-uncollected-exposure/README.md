# Tell an uncollected dose from an absent administration

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adex-uncollected-exposure.html)

**Goal:** summarize each subject-treatment combination, adding
`DOSECUM`, `NDOSREC`, and `NDOSVAL`.

**Input:** a subject-treatment inventory carrying treatment name
(`EXTRT`), with the linked exposure records carrying the
administered dose (`EXDOSE`) and the exposure sequence number
(`EXSEQ`).

**Variables:**

- `DOSECUM`: total administered dose across the linked exposure
  records; empty when no dose was ever collected, so an
  unrecorded quantity is never reported as a measured zero.
- `NDOSREC`: number of linked exposure records; empty when the
  treatment has no exposure record at all.
- `NDOSVAL`: number of linked exposure records carrying a dose;
  empty when the treatment has no exposure record at all, and
  zero when records exist but every dose was left blank.

**Note:** a treatment whose doses were all left blank stays
distinguishable from one that was never administered: the first
has exposure records and a zero `NDOSVAL`, while the second has
nothing to count and leaves all three measures empty.

**Standard:** ADaM | **Domain:** ADEX
