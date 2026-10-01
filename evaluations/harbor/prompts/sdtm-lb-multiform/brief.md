Following CDISC SDTM standards, use the provided ODM dataset to create an
LB dataset with one record per reported result from the serum,
skin-biopsy, saliva, and tape-strip forms.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, VISIT, VISITNUM, LBTESTCD, LBTEST, LBCAT,
LBSPEC, LBLOC, LBORRES, LBORRESU, LBSTRESC, LBSTRESN, LBSTRESU, LBSTAT,
LBDTC

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
