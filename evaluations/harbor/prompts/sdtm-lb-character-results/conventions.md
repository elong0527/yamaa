Following CDISC SDTM standards, use the provided ODM dataset to create an LB
dataset with one record per laboratory result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU, LBSTRESC,
LBSTRESN, LBSTRESU, LBNRIND, LBDTC

DOMAIN is "LB". LBTESTCD and LBTEST are "PROT" and "Protein" for protein,
"CK" and "Creatine Kinase" for creatine kinase, "GLUC" and "Glucose" for
glucose, "KETON" and "Ketones" for ketones, and "CREAT" and "Creatinine" for
creatinine. LBSTRESC folds the known dipstick spellings, in any letter case,
to the standardized results: "NEGATIVE" and "NEG" give "NEGATIVE", "TRACE"
and "TR" give "TRACE", "1+", "+1", and "1 PLUS" give "1+", and "2+" and "+2"
give "2+". LBNRIND is "LOW" or "HIGH", or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
