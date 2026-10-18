Following CDISC ADaM standards, use the provided VS dataset to create an ADVS
dataset with one record per collected character-result row.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VSSEQ, SERIES, AVISITN, AVALC, PREVAVALC

PREVAVALC is the closest earlier non-blank AVALC for the same subject and
series, never the row's own AVALC. Rows with no SERIES value share one
series. Within a series, rows are ordered by AVISITN, with a missing
AVISITN last, and rows sharing an AVISITN, or both missing one, keep their
collected order. Blank results are skipped, so the closest earlier
non-blank AVALC may lie beyond consecutive blank rows, but it never comes
from another series. PREVAVALC has no value when no earlier row in the
series has a non-blank AVALC.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
