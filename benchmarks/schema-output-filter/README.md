# Publish One Category of a Dataset With the Whole Dataset's Numbers

**Goal:** publish the category B records of a findings dataset as a part of
their own, each keeping the sequence number `SEQ` the whole dataset gives it.

**Input:** `input/src.csv` holds each subject's findings, one record per test,
with its category (`CAT`). Two spec files:

- `spec_full.yaml` builds the whole dataset, `obs.csv`;
- `spec_part.yaml` names `spec_full.yaml` as its parent and publishes only the
  category B records, `obs_b.csv`;
- `spec_part_unlisted.yaml` publishes the same records without the category
  variable, `obs_b_unlisted.csv`: the category still decides which records
  are kept.

**Variables:**

- `SUBJ`, `CAT`, `TEST` and `VAL` carry the subject, the category, the test
  and its value from the input record.
- `SEQ` numbers a subject's records from 1, ordered by category and then by
  test, over every category: a subject's category B records follow their
  category A records, so subject `S1`'s first B record is number 3 in both
  files.

**Note:** the part keeps completed records, after every number is assigned,
so it holds the whole dataset's numbers rather than numbering its own records
again from 1. Its keys and any checks apply to the records it keeps.
