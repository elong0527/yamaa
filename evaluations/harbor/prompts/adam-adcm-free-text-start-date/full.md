Following CDISC ADaM standards, use the provided CM dataset to create an ADCM
dataset with one record per concomitant medication.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, CMSTDAT, ASTDTC, ASTDT

The collected start date in CM, CMSTDAT, is free text in one of five spellings:
an ISO 8601 date, "YYYY-MM-DD"; "DDMONYYYY", a two-digit day, a three-letter
English month abbreviation in any letter case, and a four-digit year;
"MM/DD/YYYY", a slashed date with the month first; "YYYYMMDD", eight digits;
and "MONYYYY", a three-letter month abbreviation and a four-digit year, which
gives a month alone. ASTDTC is the analysis start as ISO 8601 text at the
precision collected, "YYYY-MM-DD" for a full date and "YYYY-MM" for a month
alone. ASTDT is the analysis start date: the collected date, or the first day
of the collected month when only a month was collected. ASTDTC and ASTDT have
no value when CMSTDAT has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adcm.csv.
