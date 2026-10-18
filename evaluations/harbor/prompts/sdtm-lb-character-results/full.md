Following CDISC SDTM standards, use the provided ODM dataset to create an LB
dataset with one record per laboratory result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU, LBSTRESC,
LBSTRESN, LBSTRESU, LBNRIND, LBDTC

DOMAIN is "LB". STUDYID is the ODM study OID and USUBJID is the ODM subject
key, each as given. A repeated form occurrence gives its own record, so a
second result for the same test and visit stays a separate record. LBSEQ
numbers each subject's records from 1 in the collection order protein,
creatine kinase, glucose, ketones, creatinine, then by form repeat. LBTESTCD
is the test short name for the collection form and LBTEST its test name:
"PROT" and "Protein" for protein, "CK" and "Creatine Kinase" for creatine
kinase, "GLUC" and "Glucose" for glucose, "KETON" and "Ketones" for ketones,
and "CREAT" and "Creatinine" for creatinine. LBSTRESC folds the known
dipstick spellings, in any letter case, to the standardized results:
"NEGATIVE" and "NEG" give "NEGATIVE", "TRACE" and "TR" give "TRACE", "1+",
"+1", and "1 PLUS" give "1+", and "2+" and "+2" give "2+". Any other
collected result is kept as collected in LBSTRESC. LBSTRESN holds the number
the standardized result states when it is a plain number such as "0.9", and
has no value for grades and for censored values. LBSTRESU is the reported
unit, with no value when the form records no unit. LBNRIND is "LOW" for a
result below the quantification limit (its standardized result begins with
"<"), "HIGH" for a result above the limit (beginning with ">"), and has no
value otherwise. LBDTC is the collection date from the same form occurrence.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
