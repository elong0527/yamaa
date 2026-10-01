Following CDISC SDTM standards, use the provided BX_RAW dataset to create
an LB dataset with one record per skin compartment a subject has.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBSPEC, LBLOC, LBORRES,
LBORRESU, LBSTRESN, LBSTAT

LBTESTCD is IL13, LBTEST is Interleukin 13, and LBSPEC is SKIN on every record.
LBSEQ numbers each subject's records with the lesional record first. LBLOC is
LESIONAL on the lesional record and NON-LESIONAL on the non-lesional record.

LBSTAT is NOT DONE or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
