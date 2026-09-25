# Build SDTM subject visits from ODM extracts

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-sv-subject-visits.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the SDTM subject visits (SV) domain from Operational Data
Model (ODM) item data plus a demographics (DM) extract, naming for each
visit the study events its subjects attended, when each visit started and
ended, and the planned day, order, and epoch it belongs to.

**Input:** an ODM extract, one row per item value with the columns StudyOID,
MetaDataVersionOID, SubjectKey, StudyEventOID, StudyEventRepeatKey,
ItemGroupOID, ItemGroupRepeatKey, ItemOID and Value; plus a small DM extract
with STUDYID, USUBJID and the reference start date RFSTDTC. Each study
event carries its planned-visit attributes (visit name, visit number,
planned day, planned order, epoch) as ODM items, and the VS, LB and EX
forms carry their own collection dates.

**Variables:**

- `SVSEQ` numbers each subject's visits in date order.
- `VISIT` is the visit name carried on the study event, e.g. Week 2.
- `VISITNUM` is the planned visit number. Distinct numbers starting at
  99.1 identify a subject's unscheduled visits.
- `VISITDY` is the planned study day of the visit; it stays empty for
  unscheduled visits, which carry no plan.
- `SVSTDTC` is the start date of the visit, the earliest form date among
  the visit's VS, LB and EX forms.
- `SVENDTC` is the end date of the visit, the latest form date; a visit
  whose forms were collected on different days spans them.
- `SVSTDY` is the study day of the visit start, counted from the
  subject's RFSTDTC.
- `SVENDY` is the study day of the visit end, counted from the subject's
  RFSTDTC.
- `TAETORD` is the planned order of the element within the arm; it stays
  empty for unscheduled visits.
- `EPOCH` is the epoch carried on the study event, e.g. TREATMENT.
- `SVUPDES` is the description the site entered for the unplanned visit,
  carried on the unscheduled study event as an ODM item; it stays empty
  for planned visits.

**Note:** visits are numbered by their start date, so an unscheduled visit
takes its place between the planned visits around it. Repeated unscheduled
visits retain separate identities.

**Provenance:** the fixtures are hand-built from a realistic EDC visit
flow with plausible ODM naming, not records from a real study.

**Standard:** SDTM | **Domain:** SV
