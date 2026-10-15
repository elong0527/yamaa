# Flag Any Reported Dose Adjustment

**Goal:** one record per subject flagging any reported dose
adjustment (`DOSADJFL`).

**Input:** subject list plus exposure (EX) records carrying the
reason for a dose adjustment (`EXADJ`), exposure as collected
(EC) records carrying the same reason (`ECADJ`), and
findings-about (FA) records carrying a test code (`FATESTCD`),
the object of the finding (`FAOBJ`), and its result
(`FASTRESC`).

**Variables:**

- `DOSADJFL` holds `Y` when any source holds a qualifying
  record: a filled `EXADJ` value, a filled `ECADJ` value, or a
  findings-about row with `FATESTCD` of `OCCUR`, `FAOBJ` of
  `DOSE ADJUSTMENT`, and `FASTRESC` of `Y`. A pre-specified
  (`PRESP`) or non-dose-adjustment finding does not qualify. It
  holds `N` when the subject has at least one record in any of
  the three sources but no qualifying record. It stays empty
  when the subject is absent from all three sources.

**Note:** a record belongs to a subject only when both the study
and subject identifiers match, and a source record for a subject
not in the subject list adds no record.
