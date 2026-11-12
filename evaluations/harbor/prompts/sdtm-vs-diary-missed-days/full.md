Following CDISC SDTM standards, use the provided DIARY_DAYS, TEMPERATURE, and
VACCINATION datasets to create a VS dataset with one record per subject per
vaccination given per day of the temperature diary period.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSTEST, VSORRES, VSORRESU, VSSTAT,
VSREASND, VSDTC, VSTPT, VSTPTNUM, VSTPTREF, VSRFTDTC

DOMAIN is "VS". STUDYID and USUBJID are as VACCINATION gives them.

Every vaccination that was given has one record for each diary day listed in
DIARY_DAYS, whether or not a temperature was entered for that day. A
vaccination with no date in VACCINATION was not given and has no records.

- VSTESTCD is "TEMP" and VSTEST is "Temperature" on every record.
- VSTPTNUM is the diary day's number and VSTPT its label, as DIARY_DAYS
  gives them ("DAY 1", ...); day 1 is the day of vaccination.
- VSTPTREF is the visit name of the vaccination the diary day follows, as
  VACCINATION gives it (such as "VACCINATION 1"), and VSRFTDTC is that
  vaccination's date.
- VSDTC is the diary date: the vaccination date on day 1, and one calendar
  day later for each later diary day.
- A temperature was entered for a diary day when TEMPERATURE holds a record
  for the same subject dated on that diary date.
- VSORRES is that temperature and VSORRESU its unit, each as text exactly as
  TEMPERATURE writes it, so a reading written with a trailing zero keeps it.
  Both have no value on a diary day with no temperature entered.
- VSSTAT is "NOT DONE" and VSREASND is "NOT ENTERED" on a diary day with no
  temperature entered; both have no value otherwise.
- VSSEQ numbers each subject's records from 1, in order of vaccination date
  and then diary day number.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
