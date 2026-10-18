Following CDISC ADaM standards, use the provided ADSL, AE, and FA datasets to
create an ADAE dataset with one record per subject per adverse event per
analysis period.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, AESEQ, AEDECOD, AESTDTC, AEENDTC, ASTDTM, AENDTM,
TRTSDTM, AESEV, AETOXGR, ASEV, ATOXGRN, TRTEMFL

A period starts at the event's onset and at each change time collected in
FA, where FALNKID matches the event's AELNKID, and the FA times are
collected effective change times used as they stand. Severity and grade
findings collected at the same time form a single period. Separate events
that share a term remain separate events. Every period of an event holds
the event's AESEQ, AEDECOD, AESTDTC, AEENDTC, AESEV, and AETOXGR. ASEQ
numbers the periods within a subject in order of AESEQ and then ASTDTM,
starting at 1. ASTDTM is the period's start. AENDTM is the next period's
start, or AEENDTC for the last period, and the last period of an ongoing
event has no AENDTM. ASEV is the severity most recently collected at or
before the period's start, and ATOXGRN is the numeric toxicity grade
(1 through 5) most recently collected at or before the period's start;
each has no value until its dimension is first collected. Severity ranks
in the order MILD, then MODERATE, then SEVERE. TRTEMFL is Y on every
period of an event that starts at or after the subject's first exposure.
For an event that starts before first exposure, TRTEMFL is Y from the
first period that starts at or after first exposure and whose severity or
grade is higher than the last pre-treatment period's severity or grade,
and on every later period of that event. A missing severity or grade on
either side of that comparison shows no increase for that dimension.
TRTEMFL has no value on any other period: improvement alone is not
treatment-emergent, and no period of an untreated subject is Y.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
