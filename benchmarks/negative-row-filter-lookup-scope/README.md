# Reject a Diary Selection Checked Against Another Dataset's Date

**Goal:** keep the diary entries of randomized subjects, with the entry date
in `QSDTC` and the diary score in `QSSTRESN`.

**Input:** diary entries carrying the entry date (`DIARYDT`) and score
(`SCORE`), a randomization list carrying each subject's randomization date
(`RANDDT`) and arm (`ARM`), and demographics carrying each subject's
reference start date (`RFSTDTC`).

**Variables:**

- `QSDTC` would be the diary entry date.
- `QSSTRESN` would be the diary score.

**Note:** an entry is kept when the subject was randomized on or before a
date, and that date is taken from the demographics records. The diary
entries, not the demographics records, build the rows, so an entry has no
demographics record of its own to compare with. The run is rejected before
any data is read and no artifact is accepted.

## How to fix

Compare the randomization date with a date of the diary entry itself, here
the entry date:

```yaml
intermediates:
  - id: RANDOMIZED
    dataset: RAND
    key: [USUBJID]
    filter: "RAND.RANDDT <= DIARY.DIARYDT"
    no_match: null
```
