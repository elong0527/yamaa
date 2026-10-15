# Worst Severity Flags

**Goal:** rank severity and flag the worst-severity
treatment-emergent event for each subject and preferred term
(`AEDECOD`), adding `ASEVN` and `AOCCPIFL`.

**Input:** adverse event (AE) records carrying body system or organ
class (`AEBODSYS`), dictionary-derived term (`AEDECOD`), analysis
start date (`ASTDT`), severity (`AESEV`), and treatment-emergent
flag (`TRTEMFL`).

**Variables:**

- `ASEVN`: numeric rank of `AESEV`, `1` for MILD through `3` for
  SEVERE; blank when no severity was collected.
- `AOCCPIFL`: `Y` on the eligible event with the greatest `ASEVN`
  for the subject and preferred term (`AEDECOD`); blank otherwise.
  Ties break by earliest `ASTDT`, then lowest `AESEQ`, so exactly
  one event per term is flagged.

**Note:** only a treatment-emergent event with a graded severity is
eligible, so a preferred term (`AEDECOD`) whose events are all
ineligible has no flagged event.
