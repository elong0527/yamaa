Following CDISC ADaM standards, use the provided ADSL and ADAE datasets to
create an ADRISK dataset with one record per subject per prespecified event
term, and an ADINC summary with one record per event term per actual
treatment.

The ADRISK output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, AEDECOD, FIRSTDT, EVENTN, RISKDAYS
The ADINC output dataset should contain the following columns in this order:
STUDYID, TRT01A, AEDECOD, NSUBJ, PERSONDAYS, PERSONYRS, EAIR100

Read the source datasets from /app/input and save the completed datasets as
/app/output/adrisk.csv and /app/output/adinc.csv.
