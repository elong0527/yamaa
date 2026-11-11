# Replicate Readings

**Goal:** keep every blood pressure reading taken at a visit as its own
record, with `VSSPID` numbering the readings of one test at one visit.

**Input:** long-form Operational Data Model (ODM) rows, one row per
collected item, with the collected entry in the value field. Each
replicate of a blood pressure reading is one repeat of the
vital-signs item group, numbered in the item-group repeat key; the
visit rides in the study event.

**Variables:**

- `VSTESTCD` is the test code of the collected item: `SYSBP` or
  `DIABP`.
- `VSTEST` is the test name of the collected item.
- `VSSPID` is the item-group repeat key, the sponsor's identifier of
  the reading: it numbers the readings of one test at one visit.
- `VSORRES` is the reading written as text from its numeric value, so
  trailing zeros are dropped (a collected `82.50` reads `82.5`).
- `VSORRESU` is `mmHg` for every record.
- `VSSTRESN` is the numeric reading.
- `VSSTRESC` is the same value written as text, so it always agrees
  with the numeric result.
- `VSSEQ` numbers every record of a subject in one sequence, by
  visit, then test code (`DIABP` before `SYSBP`), then the readings
  in the order they were taken.

**Note:** a reading that was not taken, whether its record is absent
or its value is blank, gives no record, and the other readings keep
their numbers. No visit mean is added: averaging the readings is an
analysis step, made in the analysis dataset.
