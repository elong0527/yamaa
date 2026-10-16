# Derive the Change on Standing and Orthostatic Flags

**Goal:** for each subject and visit, derive the change on standing in
systolic blood pressure (`SYSBPOC`), diastolic blood pressure (`DIABPOC`)
and heart rate (`HROC`), and flag orthostatic hypotension (`OHYPO`) and
orthostatic tachycardia (`OTACHY`).

**Input:** collected vital signs with systolic (`SYSBP`) and diastolic
(`DIABP`) blood pressure in mmHg and heart rate (`HR`) in beats/min, taken
supine and then standing at the same visit; the position is in `VSPOS` and
the planned time point in `VSTPT`: a standing reading after three minutes
and, at some visits, an earlier one after one minute.

**Variables:**

- `PARAMCD` and `PARAM` name the change or the flag the record holds.
- `AVAL`: on a change record, the test's reading after three minutes
  standing minus its supine reading at the same visit, not rounded, so a
  fall is negative. The reading after one minute is never used. No change
  record is added when either reading is missing or was not taken. Empty on
  a flag record.
- `AVALC`: on an `OHYPO` record, `Y` when systolic pressure falls by 20
  mmHg or more or diastolic pressure falls by 10 mmHg or more, either fall
  being enough even when the other change is missing; `N` when both changes
  are present and neither falls that far. On an `OTACHY` record, `Y` when
  heart rate rises by 30 beats/min or more and `N` when it rises less.
  Empty on a change record.

**Note:** each threshold includes its boundary, so a fall of exactly 20 or
10 mmHg, or a rise of exactly 30 beats/min, meets it. A visit gets no
`OHYPO` record when neither change meets its threshold but one of them is
missing, and no `OTACHY` record when the heart rate change is missing. A
test with more than one supine reading, or more than one reading after three
minutes standing, at one visit stops the run rather than choosing one.
