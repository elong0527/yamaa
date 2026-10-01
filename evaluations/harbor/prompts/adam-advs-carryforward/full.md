Following CDISC ADaM standards, use the provided PLAN, VS, and ADSL
datasets to create an ADVS dataset with one record per planned
measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, VSSEQ, PARAMCD, ADT, AVAL, TRTSDT, HEIGHTBL

ADT is the planned analysis date, carried through unchanged. A
collected record belongs to a planned measurement when the test code
matches the planned parameter and the collection date matches ADT;
VSSEQ is that record's sequence number and has no value when the
planned measurement was not collected.

AVAL is the collected numeric result when the planned measurement was
collected, otherwise the most recent earlier collected value for the
same subject and parameter; it has no value before the first collected
value. A carried value never crosses subjects or parameters.

TRTSDT is the subject treatment start date, repeated on every record
of the subject. HEIGHTBL is the latest height collected on or before
treatment start, repeated on every record of the subject so later
weight records retain it; it has no value for a subject with no
pre-treatment height.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
