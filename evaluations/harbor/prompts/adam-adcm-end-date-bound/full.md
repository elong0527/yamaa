Following CDISC ADaM standards, use the provided CM and ADSL datasets to
create an ADCM dataset with one record per concomitant medication.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, CMENDTC, AENDT, AENDTF, DTHDT

AENDT is the analysis end date, from CMENDTC. A fully collected date is
used as collected, even when it falls after the date of death. A year and
month is completed to the last day of that month, and a year alone to
December 31 of that year. A missing CMENDTC gives no AENDT. DTHDT is the
subject's date of death from ADSL, empty for a subject who has not died.
A supplied end is never placed after a death that falls inside the
collected month or year: when it would fall after DTHDT and the collected
month or year contains DTHDT, AENDT is DTHDT; when the whole collected
month or year lies after DTHDT, AENDT stays on the last day of that month
or year. AENDTF is D when only the day was supplied (a year and month was
collected) and M when the month and day were supplied (a year alone was
collected). It has no value when the date was collected in full and when
AENDT has no value. Dates are written as YYYY-MM-DD.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adcm.csv.
