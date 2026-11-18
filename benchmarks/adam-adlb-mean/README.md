# Add each subject's average record

**Goal:** add one average record (`DTYPE` `AVERAGE`) per subject and
laboratory parameter, whose analysis value (`AVAL`) is the mean of the
subject's collected results, and number every record (`ASEQ`).

**Input:** collected laboratory (LB) records carrying study,
subject, sequence number, test code, and the collected numeric
result.

**Variables:**

- `ASEQ`: the record's number within the subject: the collected
  records of a parameter in sequence order, then its average record.
- `LBSEQ`: the collected record's sequence number; empty on an
  average record.
- `AVAL`: on a collected record, the collected numeric result,
  empty when no usable result was collected; on an average record,
  the mean of the subject's non-missing results for the parameter.
- `DTYPE`: `AVERAGE` on an average record; empty on a collected
  record.

**Note:** the average is taken separately for each test code. A
subject with no non-missing result for a parameter gets no average
record, rather than one with an empty or zero value.
