Following CDISC SDTM standards, use the provided ODM dataset to create an
LB dataset with one record per collected calcium and creatinine result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU,
LBSTRESN, LBSTRESU, LBSTAT, LBDTC

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

LBSEQ numbers the records within a subject by collection date, then test
code. LBTESTCD is CA for calcium rows and CREAT for creatinine rows;
LBTEST is Calcium when the test code is CA and Creatinine when it is
CREAT.

LBORRES is the collected entry, kept exactly as reported, and has no value
when the test was not done. LBORRESU and LBSTRESU are mg/dL for every
record with a result, and have no value when the test was not done.
LBSTRESN is the numeric form of the collected entry, and has no value when
the entry is text rather than a number or when the test was not done.
LBSTAT is NOT DONE when the collected entry says the test was not done,
and has no value otherwise. LBDTC is the collection date from the same
visit group.

A test with no collected entry produces no record. An entry of NOT DONE
does produce a record, with no value in the result and unit fields.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
