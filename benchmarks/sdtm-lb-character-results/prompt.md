Following CDISC SDTM standards, use the provided ODM dataset to create an
LB dataset with one record per collected laboratory result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU,
LBSTRESC, LBSTRESN, LBSTRESU, LBNRIND, LBDTC

LBSEQ numbers each subject's records in collection order (protein,
creatine kinase, glucose, ketones, creatinine), then by form repeat. The
test code and name come from the collection form: PROT and Protein, GLUC
and Glucose, KETON and Ketones, CREAT and Creatinine, or CK and Creatine
Kinase.

LBORRES is the result as collected. LBORRESU is the reported unit, and has
no value when the form records no unit. LBSTRESC folds known dipstick
spellings, in any letter case, to NEGATIVE, TRACE, 1+, or 2+; anything else
is kept as collected. LBSTRESN holds a number only for a true numeric
result, and has no value for grades and censored values. LBSTRESU repeats
the reported unit. LBNRIND is LOW for a result below the quantification
limit and HIGH for one above it, and has no value otherwise. LBDTC is the
collection date from the same form occurrence.

A form with no reported result produces no record; a repeated form
occurrence keeps a second result for the same test and visit separate. A
grade spelling outside the known spellings is kept as collected.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
