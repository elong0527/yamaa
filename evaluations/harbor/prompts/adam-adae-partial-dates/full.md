Following CDISC ADaM standards, use the provided ADSL and AE datasets to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, ASTDT, ASTDTC, ASTDTF, TRTSDT,
TRTEMFL

ASTDT is the collected start date as it stands when AESTDTC is a full
date. A full date is left exactly as collected even when it falls
before TRTSDT. When AESTDTC holds a year and a month only, ASTDT is
the 15th of that month. When that 15th falls before TRTSDT and
TRTSDT falls in the same month, ASTDT is TRTSDT instead. When the
whole of that month falls before TRTSDT, ASTDT has no value. ASTDT
has no value when AESTDTC holds a year only, holds text that is not
a date, or has no value. When TRTSDT has no value, a year and a
month is still completed to the 15th. ASTDTC is ASTDT written as
text in the form "YYYY-MM-DD", and ASTDTC has no value when ASTDT
has no value. ASTDTF is "D" when the day of ASTDT was supplied to
complete a year and a month, and ASTDTF has no value when AESTDTC
is a full date or when ASTDT has no value. TRTEMFL is "Y" when
ASTDT falls on or after TRTSDT. A supplied day counts for TRTEMFL
exactly as a collected day would. TRTEMFL has no value when ASTDT
has no value, when ASTDT falls before TRTSDT, or when TRTSDT has
no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
