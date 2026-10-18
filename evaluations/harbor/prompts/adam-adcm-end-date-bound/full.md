Following CDISC ADaM standards, use the provided ADSL and CM datasets to create
an ADCM dataset with one record per concomitant medication.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, CMENDTC, AENDT, AENDTF, DTHDT

AENDT is the analysis end date, formed from the collected end date. A
collected end date given in full is used exactly as collected, even when
it falls after the subject's date of death. A collected end date that
gives a year and month is completed to the last day of that month. A
collected end date that gives a year alone is completed to December 31
of that year. A missing collected end date gives AENDT no value. When
the completed end date falls after the subject's date of death, DTHDT,
but the collected month or year also contains DTHDT, AENDT is DTHDT.
When the whole collected month or year begins after DTHDT, AENDT stays
on the last day of that month or year. AENDTF is "D" when only the day
was supplied and "M" when the month and day were supplied. AENDTF has
no value when the collected end date was given in full and when AENDT
has no value. AENDTF still records what was supplied when AENDT moves
back to the date of death.
DTHDT is the subject's date of death from ADSL, and has no value when
the subject has not died.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adcm.csv.
