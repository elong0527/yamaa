Following CDISC ADaM standards, use the provided ADSL and ADAE datasets to
create an ADRISK dataset with one record per subject per prespecified event
term, and an ADINC summary with one record per event term per actual
treatment.

The ADRISK dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, AEDECOD, FIRSTDT, EVENTN, RISKDAYS
The ADINC dataset should contain the following columns in this order:
STUDYID, TRT01A, AEDECOD, NSUBJ, PERSONDAYS, PERSONYRS, EAIR100

The prespecified event terms are "DIARRHOEA" and "HEADACHE". The collection
window ends 30 days after TRTEDT. RISKDAYS counts both first dose and the
risk endpoint. EVENTN is 1 or 0. PERSONYRS uses 365.25 days per year;
EAIR100 is per 100 person-years. PERSONYRS and EAIR100 are not rounded.

Read the source datasets from /app/input and save the completed ADRISK dataset
as /app/output/adrisk.csv and the completed ADINC dataset as
/app/output/adinc.csv.
