Following CDISC ADaM standards, use the provided ADAE and ADSL datasets to
create an ADRISK dataset with one record per study and subject per
event term and an ADINC dataset with one record per study and actual
treatment per event term.

The ADRISK dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, AEDECOD, FIRSTDT, EVENTN, RISKDAYS
The ADINC dataset should contain the following columns in this order:
STUDYID, TRT01A, AEDECOD, NSUBJ, PERSONDAYS, PERSONYRS, EAIR100

- ADRISK includes both prespecified AEDECOD terms, "DIARRHOEA" and
  "HEADACHE", once for each subject, including a term the subject never
  experienced.
- The collection window for each subject runs from the subject's first
  dose date through 30 days after the subject's last dose date.
- For each subject and term, FIRSTDT is the earliest onset date of that
  term in the collection window, and FIRSTDT has no value when no onset
  of that term falls in the window.
- EVENTN is 1 when FIRSTDT has a value and 0 when FIRSTDT has no value.
- RISKDAYS is the number of days from the first dose date through
  FIRSTDT, counting both dates, when EVENTN is 1.
- RISKDAYS is the number of days from the first dose date through the
  collection-window end, counting both dates, when EVENTN is 0.
- An onset before the first dose date or after the collection-window
  end is not eligible and does not shorten RISKDAYS.
- In ADINC, NSUBJ is the number of subjects with an EVENTN of 1 for the
  same study, actual treatment, and term.
- PERSONDAYS is the sum of RISKDAYS over all subjects in the same
  study, actual treatment, and term, including subjects with an EVENTN
  of 0.
- PERSONYRS is PERSONDAYS divided by 365.25, and PERSONYRS is not
  rounded.
- EAIR100 is 100 times NSUBJ divided by PERSONYRS, and EAIR100 is not
  rounded.

Read the source datasets from /app/input and save the completed ADRISK dataset
as /app/output/adrisk.csv and the completed ADINC dataset as
/app/output/adinc.csv.
