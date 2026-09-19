# Derive EX from collected exposure

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ex-from-ec.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** draft - first commit.

**Goal:** build one Exposure (EX) record per administered dose from
collected exposure records, converting every collected form to the
dose in protocol-specified units and naming the treatment.

**Input:** collected exposure rows in three collected forms: tablet
counts with strength, a weight-based infusion dose in mg/kg with body
weight, and blinded kit numbers resolved through a kit-to-treatment
list. Each row says whether the dose was taken.

**Variables:**

- `EXSEQ` numbers the subject's administrations in the order they
  happened, by start date then treatment. With `STUDYID` and `USUBJID`
  it identifies the record.
- `EXTRT` is the actual treatment: the collected name for tablets and
  infusions, and the kit list's treatment for blinded kits.
- `EXDOSE` is the administered dose in milligrams: tablets times
  strength; the mg/kg dose times body weight for infusions; the kit
  list's dose for blinded kits. A placebo kit carries 0.
- `EXDOSU` is always `mg`: every collected form converts to the
  protocol-specified unit.
- `EXSTDTC` is when the administration started, as collected.
- `EXENDTC` is when the administration ended, as collected.

Note: a collected dose marked not taken never reaches EX, so
`EXSEQ` numbers only administrations that happened.

Provenance: EC/EX follow the CDISC SDTMIG exposure-as-collected
assumptions, and the conversion pattern follows the SDTMIG v3.3
exposure example (collected administrations in EC, one EX record in
protocol-specified units). All subjects, dates, doses, weights and
kit numbers are invented fixtures.

**Standard:** SDTM | **Domain:** EX
