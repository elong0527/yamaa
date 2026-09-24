# Boolean Substring Flags

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-str-contains.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag adverse events whose dictionary-derived term contains
one of the dermatologic criterion terms (`APPLICATION`,
`DERMATITIS`, `ERYTHEMA`, or `BLISTER`), the kind of term search
behind a customized query name such as `CQ01NAM`.

**Input:** one record per adverse event carrying `AESEQ` (sequence
number) and `AEDECOD` (dictionary-derived term).

**Variables:**

- `CRIT1FL`: `Y` when the term contains any of the four criterion
  terms anywhere in its text, as `APPLICATION SITE DERMATITIS` does;
  missing otherwise.
- `DERMFL`: `Y` when the term contains `DERM` (as in `DERMATITIS`)
  or `ERYTHEMA` anywhere in its text; missing otherwise.

**Note:** each flag is `Y` or blank, never `N`. An event with no
recorded term is blank as well: its search is unknown rather than a
non-match, and only a match sets the flag.

**Standard:** ADaM | **Domain:** ADAE
