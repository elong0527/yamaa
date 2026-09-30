Following CDISC ADaM standards, use the provided VS dataset to create an
ADVS dataset with one record per vital-signs record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VSSEQ, SERIES, AVISITN, AVALC, PREVAVALC

SERIES identifies the analysis series; rows with no series value share
one series. AVISITN is the visit number that orders rows within a
series. AVALC is the current character result, kept as collected, and
has no value when no result was collected.

PREVAVALC is the closest earlier result with a value for the same
subject and series, never the row's own; it has no value when no
earlier row in the series has a result. Within a series, rows are
ordered by visit number with missing numbers last, and rows sharing a
visit number, or both missing one, keep their collected order. The
look-back skips blank results but never crosses into another series.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
