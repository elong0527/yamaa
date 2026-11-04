# Study-Level Values Read From a One-Record Settings Table

**Goal:** one row per subject carrying the study identifier (`STUDYID`) and the
trial phase (`PHASE`), both taken from the study's settings.

**Input:** one `spec.yaml` over subject records, which carry the subject and
their age (`AGE`) but no study column, and a study settings table holding one
record: the study identifier, its title and its phase.

**Variables:**

- `STUDYID` is the study identifier from the settings record, the same on
  every row.
- `PHASE` is the trial phase from the settings record, the same on every row.

**Note:** every subject reads the settings table's one record, though the
two tables share no column. A settings table with no record, or with more
than one, stops the run rather than leaving the values empty or choosing a
record.
