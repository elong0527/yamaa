Following CDISC ADaM standards, use the provided CM dataset to create an ADCM
dataset with one record per concomitant medication, identified by USUBJID and
CMSEQ.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, CMSTDAT, ASTDTC, ASTDT

The sites recorded the collected start, CMSTDAT, as free text in one of five
spellings:
- an ISO 8601 date, such as "2003-05-06";
- a two-digit day, a three-letter English month abbreviation and a four-digit
  year, such as "06MAY2003", the month in any letter case, so "06may2003" is
  the same date;
- a slashed date, which reads month first: "05/06/2003" is 6 May 2003;
- eight digits, year then month then day, such as "20030506";
- a three-letter month abbreviation and a four-digit year, such as "MAY2003",
  which records a month alone.

ASTDTC is the start as ISO 8601 text at the precision it was collected:
"YYYY-MM-DD" for a full date, such as "2003-05-06", and "YYYY-MM" for a month
alone, such as "2003-05". ASTDT is the analysis start date: the collected
date, or the first day of the month when only a month was collected. ASTDTC
and ASTDT have no value when CMSTDAT has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adcm.csv.
