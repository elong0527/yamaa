# Exposure from Collected Records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ex-from-ec.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the administered exposure records from the exposure
as collected: tablet counts, a weight-based dose, a blinded kit, and
an AUC target, adding `EXSEQ`, `EXTRT`, `EXDOSE`, `EXDOSU`,
`EXSTDTC`, and `EXENDTC`.

**Input:** long-form Operational Data Model (ODM) data with one row
per collected item, carrying the study, subject, visit, form, and
form repeat (`StudyOID`, `SubjectKey`, `StudyEventOID`, `FormOID`,
`FormRepeatKey`), the item (`ItemOID`), and the stored value
(`Value`). Exposure items live on the `FO.EC` form
(`IT.EC.ECOCCUR`, `IT.EC.ECTRT`, `IT.EC.FORM`, `IT.EC.TABLETS`,
`IT.EC.STRENGTHMG`, `IT.EC.DOSEMKG`, `IT.EC.AUCTARGET`,
`IT.EC.KIT`, `IT.EC.ECSTDTC`, `IT.EC.ECENDTC`); body weight is the
`IT.VS.WEIGHT` item on the `FO.VS` form at the same visit. The kit
list comes from the interactive response technology (IRT) system,
not electronic data capture (EDC), so it stays a separate input
(`input/kit_list.csv`) mapping kit numbers to treatments and doses.

**Variables:**

- `EXSEQ` is the order of the administration within the subject,
  numbered by start date and then by the form's repeat number within
  the visit.
- `EXTRT` is the administered treatment: the kit list's treatment
  for a blinded kit, otherwise as collected.
- `EXDOSE` is the administered dose: tablets times strength, the
  `mg/kg` dose times the visit's body weight, the kit list's dose
  for a blinded kit, and the AUC target as collected.
- `EXDOSU` is `mg`, except the AUC target keeps `AUC`.

**Note:** only a form whose occurrence item (`IT.EC.ECOCCUR`) is `Y`
gives an exposure record, so a skipped dose leaves none. The placebo
kit keeps its zero dose.

**Standard:** SDTM | **Domain:** EX
