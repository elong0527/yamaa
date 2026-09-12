# ADaM ADAE: flag the worst-severity event per preferred term

This example uses a pre-classified ADAE slice and a `yamaa` specification to
derive one row per adverse event:

- `AEBODSYS` and `AEDECOD` are the body system and preferred term the event was
  coded to, `ASTDT` is its start date, and `TRTEMFL` says whether it is
  treatment-emergent;
- `AESEV` is the collected severity and `AESEVN` its rank, `1` for mild through
  `3` for severe. Severity has no order of its own, so the rank is what makes
  one event worse than another;
- `AWSEVFL` marks, for each subject and preferred term, the treatment-emergent
  event of greatest severity. Events tied on severity are settled by the
  earliest start date and then the lower sequence number, so exactly one event
  per term is marked.

Only treatment-emergent events with a graded severity are eligible, so an
event that is neither is never marked, and a preferred term whose events are
all ineligible has no marked event at all.

Exactly one event is marked even when several tie at the worst severity on the
same day. That suits a flag meant to identify a single record, and not a study
that wants every event tied at the worst severity marked.
