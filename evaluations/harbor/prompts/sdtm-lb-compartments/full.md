Following CDISC SDTM standards, use the provided BX_RAW dataset to create
an LB dataset with one record per skin compartment a subject has.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBSPEC, LBLOC, LBORRES,
LBORRESU, LBSTRESN, LBSTAT

LBTESTCD is IL13, LBTEST is Interleukin 13, and LBSPEC is SKIN on every
record. LBSEQ numbers each subject's records with the lesional record
first. LBLOC is LESIONAL on the lesional record and NON-LESIONAL on the
non-lesional record; the lesional record is built only when a cohort is
recorded and is not NONAD, while every subject gets the non-lesional
record.

LBORRES is the collected result from the matching compartment, and has no
value when the expected sample was not analysed. LBORRESU is the collected
unit on every record, including records with no result. LBSTRESN is the
numeric form of the reported result, and has no value when the reported
result is blank. LBSTAT is NOT DONE when the reported result is blank, and
has no value otherwise.

A blank result with NOT DONE means the compartment exists but its sample
was not analysed, while a missing lesional record means the subject has no
lesional compartment.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
