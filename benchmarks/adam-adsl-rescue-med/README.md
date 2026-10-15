# First Rescue Medication

**Goal:** carry each subject's first rescue medication in `RESCTRT`.

**Input:** one subject-level table listing the subjects, plus
medication records carrying a treatment name (`CMTRT`), a category
(`CMCAT`), a start date (`CMSTDTC`), and a sequence number (`CMSEQ`).

**Variables:**

- `RESCTRT` is the treatment name of the subject's first rescue
  medication: a record whose category (`CMCAT`) is
  `RESCUE MEDICATION`, picked by the earliest start date
  (`CMSTDTC`), with the smaller sequence number (`CMSEQ`) breaking a
  tie. It is left blank, never filled with placeholder text, when
  the subject took no rescue medication, whether the subject took
  other medications or has no medication records at all. A rescue
  record with no recorded start date still counts; a duplicated
  record does not change the answer. A start date with only year
  and month compares as written, so it falls before any full date
  in the same month.
