# Code medications against WHODrug

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-cm-whodrug-coding.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** code reported medication names with the World Health
Organization drug dictionary (WHODrug), carrying the coder's chosen
drug record and Anatomical Therapeutic Chemical (ATC) class into CM.

**Input:** ODM item data for the medication log: each reported
medication is one repeat of the `IG.CM` item group, the reported name
is item `IT.CM.CMTRT`, and the repeat number becomes `CMSEQ`; the
coder's WHODrug coding output (chosen drug record and ATC code per
collected record); and a WHODrug dictionary extract.

**Variables:**

- `CMTRT` is the medication name as reported, kept exactly as
  written.
- `CMDECOD` is the preferred name of the drug record the coder chose
  for the reported name; blank when the verbatim is not yet coded.
- `CMCLAS` is the class name of the ATC code the coder assigned for
  this use; blank when the verbatim is not yet coded.
- `CMCLASCD` is the code of that class; blank when the verbatim is
  not yet coded.

**Note:** one drug can carry several ATC codes, and the coder assigns
the class for the use at hand: the same drug record codes to a
different `CMCLAS` for different subjects.

Standard: SDTM | Domain: CM
