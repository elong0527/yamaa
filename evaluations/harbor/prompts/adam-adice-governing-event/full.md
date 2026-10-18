Following CDISC ADaM standards, use the provided ADSL and ICE_RAW datasets to
create an ADICE dataset with one record per intercurrent event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADECOD1, ASTDT, ICEPRIO, ICERANK, GOVFL, SUPERFL,
GOVTYPE, GOVDT, GOVSEQ

ASEQ, ADECOD1, and ASTDT give the event's source sequence, type, and
complete date. ICEPRIO is 1 for ICE04 and ICE05 (death or kidney
replacement), 2 for ICE01 (prohibited medication), 3 for ICE02 (start of
other protocol-prohibited therapy), and 4 for ICE03 (treatment
discontinuation). ICERANK numbers each subject's events from 1 by
increasing priority, then earliest date, then lowest source sequence;
priority outranks chronology across event types. GOVFL is Y on position 1
and has no value elsewhere. SUPERFL is Y on every other event and has no
value on position 1. GOVTYPE, GOVDT, and GOVSEQ are the governing event's
type, date, and source sequence, on every event of the subject. A subject
without events has no record. The governing event is chosen over the
subject's completed event history; it is not the event applicable at an
earlier efficacy assessment.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adice.csv.
