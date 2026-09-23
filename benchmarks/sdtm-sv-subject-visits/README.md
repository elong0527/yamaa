# Build SDTM subject visits from ODM extracts

[![Dashboard](https://img.shields.io/badge/Dashboard-sdtm--sv--subject--visits-a4cbe8)](https://elong0527.github.io/yamaa/benchmark/sdtm-sv-subject-visits.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-fbca5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-sv-subject-visits.html)

**Goal:** build the SDTM SV domain from ODM item data plus a DM extract,
naming for each visit the study events its subjects attended, when each
visit started and ended, and the planned day, order, and epoch it belongs
to.

**Input:** an ODM extract, one row per item value with the columns StudyOID,
MetaDataVersionOID, SubjectKey, StudyEventOID, StudyEventRepeatKey,
ItemGroupOID, ItemGroupRepeatKey, ItemOID and Value; plus a small DM extract
with STUDYID, USUBJID and the reference start date RFSTDTC. Each study
event carries its planned-visit attributes (visit name, visit number,
planned day, planned order, epoch) as ODM items, and the VS, LB and EX
forms carry their own collection dates.

**Variables:**

- `STUDYID` is the study identifier from the ODM extract.
- `DOMAIN` is the domain abbreviation, always "SV".
- `USUBJID` is the unique subject identifier from the ODM extract.
- `SVSEQ` numbers each subject's visits in date order.
- `VISIT` is the visit name carried on the study event, e.g. Week 2.
- `VISITNUM` is the planned visit number; 99 marks an unscheduled visit.
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
- `SVUPDES` describes the unplanned visit; it stays empty for planned
  visits.

**Note:** the example keeps two subjects. Subject 001 attended an
unscheduled safety visit between Baseline and Week 2, and their Week 2
forms were collected across two days, so the visit spans both. Subject 002
shows a straightforward planned sequence.

**Provenance:** the fixtures are hand-built from a realistic EDC visit
flow with plausible ODM naming, not records from a real study.

**Standard:** SDTM | **Domain:** SV
