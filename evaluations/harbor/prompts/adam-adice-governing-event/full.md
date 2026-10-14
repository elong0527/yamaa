Following CDISC ADaM standards, use the provided ADSL and ICE_RAW datasets
to create an ADICE dataset with one record per intercurrent event per
subject, summarizing the governing event in the completed event history.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADECOD1, ASTDT, ICEPRIO, ICERANK, GOVFL, SUPERFL,
GOVTYPE, GOVDT, GOVSEQ

ASEQ is ICESEQ, ADECOD1 is ICETYPE and ASTDT is ICEDTC. ICEPRIO is 1 for
ICE04 and ICE05, 2 for ICE01, 3 for ICE02 and 4 for ICE03. These types are
death, kidney replacement, prohibited medication, missed visit and treatment
discontinuation, respectively. ICERANK numbers each subject's events from 1
by increasing ICEPRIO, then earliest ASTDT, then lowest ASEQ. GOVFL is "Y"
on ICERANK 1 and has no value otherwise. SUPERFL is "Y" on every other
event and has no value on ICERANK 1. GOVTYPE, GOVDT and GOVSEQ are the type,
date and source sequence of the governing event on every event of the
subject. Priority outranks chronology across event types; equal priority
uses earliest date, then lowest sequence. A subject without events has no
record. This completed-history summary does not choose the event applicable
at an earlier efficacy assessment.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adice.csv.
