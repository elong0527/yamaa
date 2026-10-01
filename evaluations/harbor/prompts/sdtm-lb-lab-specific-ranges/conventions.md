Following CDISC SDTM standards, use the provided ODM and LBRANGE datasets
to create an LB dataset with one record per collected laboratory result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, SEX, LBNAM, LBSTRESN, LBORRESU,
LBSTNRLO, LBSTNRHI, LBNRIND

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

LBSEQ is the panel repeat key. LBSTRESN is the numeric result in standard
units.

LBNRIND is LOW, HIGH, or NORMAL, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
