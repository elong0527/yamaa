Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, ASTDT, ASTDT2, REVIEWFL

ASTDT is the collected start date as it stands, and has no value when
no start date was collected. ASTDT2 is the calendar date of the
collected start datetime, and has no value when no start datetime was
collected.

REVIEWFL is Y when the event clears all three review checks, and N
otherwise:
- the review window: the collected start date falls in January 2025,
  or the collected start datetime falls at or after 09:30 on
  1 February 2025;
- the reported term begins with the literal text INF_, where the
  underscore is a literal character, so INFXREACTION does not match;
- the protocol review score is -1.5 or higher.
An event with neither a start date nor a start datetime known is N.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
