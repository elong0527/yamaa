Following CDISC ADaM standards, use the provided AE and ADSL datasets to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, ASTDT, ASTDTC, ASTDTF,
TRTSDT, TRTEMFL

ASTDT is the analysis start date. A fully collected date is used as it
stands. A collected year and month is completed to the 15th. A year
alone, non-date text, and a missing value give no analysis date. ASTDTC
is the same analysis date written as YYYY-MM-DD text, and has no value
when there is no analysis date. ASTDTF is D when the day was supplied to
complete the date, and has no value otherwise.

A supplied day is never placed before a first exposure that falls
inside the collected month. When the 15th would fall before TRTSDT, the
date moves forward to the exposure date. When the whole collected month
ends before the exposure date, the date stays on the 15th and the event
is not treatment emergent. A fully collected date stays exactly as
collected even when it falls before TRTSDT.

TRTEMFL is Y when the analysis start falls on or after TRTSDT. It has
no value when there is no analysis date, when the event started before
first exposure, or when the first-exposure date is unknown. A supplied
day counts exactly as a collected one would.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
