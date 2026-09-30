Following CDISC ADaM standards, use the provided LB and ADSL datasets
to create an ADLB dataset with one record per subject per parameter
per collection date.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ADT, TRTSDT, TRT01A, AVAL, AVALU,
ABLFL, BASE, CHG, PCHG, ASEQ

Use PARAMCD "ALT" for alanine aminotransferase, "AST" for aspartate
aminotransferase, and "ALTSI" for the ALT record converted to
International System of Units, with AVALU ukat/L on the converted
record and the collected unit otherwise. ADT is the collection date.
TRTSDT and TRT01A are the subject's treatment start date and actual
treatment.

ABLFL is Y on the latest record on or before the treatment start date
for each subject and parameter, including a result collected on the
start date itself; it has no value otherwise. A subject with a single
collected result still gets the flag.

BASE repeats the flagged baseline value on every record for the same
subject and parameter. CHG is the change from baseline. PCHG is the
change expressed as a percentage of the baseline value; it has no
value when the baseline value is zero.

A collected result with no numeric value produces no record, and a
subject with no collected results has no records.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
