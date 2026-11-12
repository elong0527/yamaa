Following CDISC SDTM standards, use the provided DIARY_DAYS, TEMPERATURE, and
VACCINATION datasets to create a VS dataset with one record per subject per
vaccination given per day of the temperature diary period.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSTEST, VSORRES, VSORRESU, VSSTAT,
VSREASND, VSDTC, VSTPT, VSTPTNUM, VSTPTREF, VSRFTDTC

DOMAIN is "VS", VSTESTCD is "TEMP" and VSTEST is "Temperature" on every
record. VSTPT is a diary day label as DIARY_DAYS gives it ("DAY 1", ...);
day 1 is the day of vaccination. VSTPTREF is a vaccination's visit name,
such as "VACCINATION 1". VSORRES is text exactly as TEMPERATURE writes it,
so a reading written with a trailing zero keeps it, or has no value. VSSTAT
is "NOT DONE" or has no value, and VSREASND is "NOT ENTERED" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
