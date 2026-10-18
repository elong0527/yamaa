Following CDISC ADaM standards, use the provided ADAE and ADSL datasets to
create an ADRISK dataset with one record per study and subject per
event term and an ADINC dataset with one record per study and actual
treatment per event term.

The ADRISK dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, AEDECOD, FIRSTDT, EVENTN, RISKDAYS
The ADINC dataset should contain the following columns in this order:
STUDYID, TRT01A, AEDECOD, NSUBJ, PERSONDAYS, PERSONYRS, EAIR100

AEDECOD is "DIARRHOEA" or "HEADACHE". RISKDAYS is the number of days,
counting both dates. PERSONYRS is not rounded, and EAIR100 is not
rounded.

Read the source datasets from /app/input and save the completed ADRISK dataset
as /app/output/adrisk.csv and the completed ADINC dataset as
/app/output/adinc.csv.
