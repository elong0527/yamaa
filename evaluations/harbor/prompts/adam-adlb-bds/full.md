Following CDISC ADaM standards, use the provided ADSL and LB datasets to create
an ADLB dataset with one record per subject per parameter per collection
date.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ADT, TRTSDT, TRT01A, AVAL, AVALU, ABLFL,
BASE, CHG, PCHG, ASEQ

The output holds one record for each collected ALT or AST result that has
a numeric value, and one further record for each such ALT result converted
to SI units. A collected result with no numeric value produces no record.
PARAMCD is "ALT" and PARAM is "Alanine Aminotransferase" for a collected
ALT result, "ALTSI" and "Alanine Aminotransferase (SI)" for a converted
record, and "AST" and "Aspartate Aminotransferase" for a collected AST
result. ADT is the collection date of the result. TRTSDT is the subject's
treatment start date, and TRT01A is the subject's actual treatment. A
subject with no collected results has no records: TRTSDT and TRT01A enrich
records that already exist and never add one. AVAL is the collected numeric
result, or the result times 0.0167 on the SI record. AVALU is the collected
unit, or "ukat/L" on the SI record. ABLFL is "Y" on the latest record on or
before the treatment start date for each subject and parameter, and has no
value otherwise. A result collected on the treatment start date itself
counts as baseline, and a subject with a single collected result for a
parameter still gets a baseline flag. BASE is the AVAL of the flagged
baseline record, repeated on every record for the same subject and
parameter. CHG is AVAL minus BASE. PCHG is 100 times CHG divided by BASE,
and has no value when BASE is zero. AVAL, CHG, and PCHG are not rounded.
ASEQ numbers the records within each subject in order of PARAMCD and then
ADT, starting at 1.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
