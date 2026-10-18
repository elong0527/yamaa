Following CDISC ADaM standards, use the provided ADSL and ADAE datasets to
create an ADRISK dataset with one record per subject per prespecified event
term, and an ADINC summary with one record per event term per actual
treatment.

The ADRISK dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, AEDECOD, FIRSTDT, EVENTN, RISKDAYS
The ADINC dataset should contain the following columns in this order:
STUDYID, TRT01A, AEDECOD, NSUBJ, PERSONDAYS, PERSONYRS, EAIR100

The prespecified event terms are "DIARRHOEA" and "HEADACHE". Every treated
subject contributes to each term, including terms they never experience.
The collection window starts at TRTSDT and ends 30 days after TRTEDT,
including both dates. FIRSTDT is the earliest ASTDT for that subject and
term within this window, or has no value if none qualifies. EVENTN is 1
when FIRSTDT has a value and 0 otherwise. RISKDAYS counts both first dose
and FIRSTDT, or both first dose and collection-window end when FIRSTDT has
no value. Repeated events contribute only the first eligible onset. An
onset before first dose or after window end contributes no event and does
not shorten time at risk. NSUBJ is the number of subjects with at least one
eligible event within each actual treatment and term. PERSONDAYS is their
total term-specific risk days, including event-free subjects. PERSONYRS is
PERSONDAYS divided by 365.25. EAIR100 is NSUBJ per 100 person-years.
PERSONYRS and EAIR100 are not rounded.

Read the source datasets from /app/input and save the completed ADRISK dataset
as /app/output/adrisk.csv and the completed ADINC dataset as
/app/output/adinc.csv.
