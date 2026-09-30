Following CDISC ADaM standards, use the provided ADLB_RAW dataset to
create an ADLB dataset with one record per laboratory record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ASEQ, AVISIT, AVAL, ANRLO, ANRHI,
ANRIND, ABLFL, BASE, BNRIND, SHIFT1, R2BASE, CRIT1, CRIT1FL

ANRIND is the record's own mark: LOW below the lower limit, HIGH above
the upper limit, and NORMAL between them, limits included, so a result
exactly at a limit reads NORMAL. It has no value when the result or
either limit is missing.

BASE repeats the flagged baseline record's value on every record of
the subject and parameter; it has no value when no record carries the
flag. BNRIND repeats the baseline record's mark the same way; it has
no value with no flagged baseline, or when the baseline record itself
could not be marked.

SHIFT1 joins the baseline mark and the record's own mark, baseline
first, so a result that stayed normal reads NORMAL to NORMAL. It has
no value when either mark is missing.

R2BASE is the record's value as a multiple of the baseline value, so
the baseline record itself reads 1 when the ratio can be computed. It
has no value when the record's value is missing, when there is no
baseline, or when the baseline is zero.

CRIT1 states the criterion the record was assessed against, a result
greater than three times the upper limit of normal (ULN): Result
greater than 3 x ULN. It has no value when the result or the upper
limit is missing. CRIT1FL says whether the record met it, Y or N; it
has no value where it could not be assessed. A result exactly at three
times the limit does not meet it. The criterion text and its flag
always arrive together, as do the mark and the shift. A record that
breaks these pairings stops the run, and no output is written.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
