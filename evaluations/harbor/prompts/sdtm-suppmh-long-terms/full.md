Following CDISC SDTM standards, use the provided MH_RAW dataset to create a
SUPPMH dataset with one record per medical history record per continuation
of its reported term.

The output dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL

The reported term MHTERM is split into pieces of at most 200 characters,
each ending after the last whole word that fits, so no word is broken
across two pieces; the space at each break belongs to neither piece. A word
that ends exactly at the 200th character fits. The first piece stays on the
MH record itself, so a term of 200 characters or fewer has no SUPPMH record.
Each SUPPMH record continues one term:

- RDOMAIN is "MH" and IDVAR is "MHSEQ".
- IDVARVAL is the parent record's MHSEQ, written as text.
- QNAM is "MHTERM1" for the second piece, with QLABEL "Reported Term for
  the Medical History 1", and "MHTERM2" for the third piece, with QLABEL
  "Reported Term for the Medical History 2". A term with two pieces has
  MHTERM1 only, and a term with three has both.
- QVAL is the piece the record continues.
- QORIG is "Collected".
- QEVAL has no value, since a collected value is not an assessment.

Read the source datasets from /app/input and save the completed dataset as
/app/output/suppmh.csv.
