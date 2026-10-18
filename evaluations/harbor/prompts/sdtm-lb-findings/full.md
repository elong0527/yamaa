Following CDISC SDTM standards, use the provided ODM dataset to create an LB
dataset with one record per collected lab result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU, LBSTRESN,
LBSTRESU, LBSTAT, LBDTC

DOMAIN is "LB". STUDYID is the ODM study OID and USUBJID the ODM
subject key, each as given. The records are only the results actually
reported: a test with no collected entry gives no record. An entry of
"NOT DONE" does give a record. LBTESTCD is "CA" for a calcium record
and "CREAT" for a creatinine record. LBTEST is "Calcium" when LBTESTCD
is "CA" and "Creatinine" when it is "CREAT". LBORRES is the collected
entry, kept exactly as reported, and has no value when the entry is
"NOT DONE". LBORRESU is "mg/dL" for every record with a result and has
no value when the test was not done. LBSTRESN is the numeric form of
the collected entry and has no value when the entry is text rather
than a number, or when the test was not done. LBSTRESU is "mg/dL" for
every record with a result and has no value when the test was not
done. LBSTAT is "NOT DONE" when the collected entry says the test was
not done and has no value otherwise. LBDTC is the collection date from
the same visit group. LBSEQ numbers the records within a subject from
1 by collection date, then by test code.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
