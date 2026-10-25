Following CDISC SDTM standards, use the provided MH_RAW dataset to create a
SUPPMH dataset with one record per medical history record per continuation
of its reported term.

The output dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL

The reported term MHTERM is split into pieces of at most 200 characters,
each ending after the last whole word that fits, so no word is broken
across two pieces.

- RDOMAIN is "MH" and IDVAR is "MHSEQ".
- IDVARVAL is written as text.
- QNAM is "MHTERM1" with QLABEL "Reported Term for the Medical History 1",
  or "MHTERM2" with QLABEL "Reported Term for the Medical History 2".
- QORIG is "Collected".
- QEVAL has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/suppmh.csv.
