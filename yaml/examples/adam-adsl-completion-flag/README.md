# ADaM ADSL: flag the subjects who completed the study

This example uses a subject-level dataset and the disposition dataset with a
`yamaa` specification to derive one row per subject:

- `TRTSDT` is the date of the subject's first dose, carried across. A subject
  never dosed has none;
- `COMPFL` is `Y` when the subject has a disposition record whose standardized
  outcome is a completion, and `N` otherwise. A subject whose only disposition
  record reports a discontinuation reason, and a subject with no disposition
  record at all, are both `N`.

The flag answers whether a completion record exists, not whether that record's
date was collected or how a later follow-up period ended. A subject who both
completed and later discontinued therefore keeps `Y` for the completion.
