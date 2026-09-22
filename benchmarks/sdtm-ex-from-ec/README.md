# Exposure from Collected Records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ex-from-ec.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the administered exposure records from the exposure
as collected: tablet counts, a weight-based dose, a blinded kit, and
an AUC target, adding `EXSEQ`, `EXTRT`, `EXDOSE`, `EXDOSU`,
`EXSTDTC`, and `EXENDTC`.

**Input:** collected exposure records carrying the dose in the form
it was collected (tablets taken with tablet strength, a dose in
`mg/kg`, an AUC target, a blinded kit number), plus a kit list
mapping kit numbers to treatments and per-visit body weight records.

**Variables:**

- `EXSEQ` is the order of the administration within the subject,
  numbered by start date then collection sequence.
- `EXTRT` is the administered treatment: the kit list's treatment
  for a blinded kit, otherwise as collected.
- `EXDOSE` is the administered dose: tablets times strength, the
  `mg/kg` dose times the visit's body weight, the kit list's dose
  for a blinded kit, and the AUC target as collected.
- `EXDOSU` is `mg`, except the AUC target keeps `AUC`.
- `EXSTDTC` is the administration start as collected.
- `EXENDTC` is the administration end as collected.

**Note:** the skipped tablet dose (`ECOCCUR = 'N'`) leaves no
exposure record - only administered treatments appear. The placebo
kit keeps its zero dose.

**Standard:** SDTM | **Domain:** EX
