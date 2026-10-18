Following CDISC ADaM standards, use the provided ADAE_RAW dataset to create an
ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEBODSYS, AEDECOD, ASTDT, TRTEMFL, AOCCFL, AOCCSFL,
AOCCPFL

An event is treatment-emergent when TRTEMFL is "Y", and only such
events are eligible to be flagged: an event that is not
treatment-emergent is never flagged at any level, and a subject with
no treatment-emergent events has no value for AOCCFL, AOCCSFL, or
AOCCPFL on every record. AOCCFL is "Y" for the subject's first
treatment-emergent event and has no value on every other record.
AOCCSFL is "Y" for the subject's first treatment-emergent event
within each body system and has no value on every other record.
AOCCPFL is "Y" for the subject's first treatment-emergent event
within each dictionary-derived term, counted inside its body system,
and has no value on every other record. The first event at each level
is the treatment-emergent event with the earliest ASTDT, and the
lower AESEQ breaks a tie between events on the same date. A subject's
first treatment-emergent event is also the first in its body system
and the first in its dictionary-derived term, so all three flags are
"Y" on that record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
