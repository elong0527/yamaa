Following CDISC ADaM standards, use the provided ADAE_RAW dataset to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEBODSYS, AEDECOD, ASTDT, TRTEMFL, AOCCFL,
AOCCSFL, AOCCPFL

Flag the first treatment-emergent event at three levels. Only an event
with TRTEMFL Y is eligible:
- AOCCFL is Y for the subject's first treatment-emergent event.
- AOCCSFL is Y for the subject's first treatment-emergent event in
  each body system.
- AOCCPFL is Y for the subject's first treatment-emergent event for
  each dictionary-derived term within its body system.
First means the earliest analysis start date, with the lower AESEQ
breaking ties on the same day. All three flags have no value on any
other event, and on every event of a subject with no
treatment-emergent events. The levels nest: the subject's first event
is also the first in its body system and term.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
