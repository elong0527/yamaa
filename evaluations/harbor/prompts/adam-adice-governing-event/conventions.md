Following CDISC ADaM standards, use the provided ADSL and ICE_RAW datasets to
create an ADICE dataset with one record per intercurrent event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADECOD1, ASTDT, ICEPRIO, ICERANK, GOVFL, SUPERFL,
GOVTYPE, GOVDT, GOVSEQ

ICEPRIO is 1 for ICE04 and ICE05 (death or kidney replacement), 2 for ICE01
(prohibited medication), 3 for ICE02 (start of other protocol-prohibited
therapy), and 4 for ICE03 (treatment discontinuation). ICERANK numbers each
subject's events from 1 by increasing priority, then earliest date, then
lowest source sequence; priority outranks chronology across event types.
GOVFL is Y or has no value. SUPERFL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adice.csv.
