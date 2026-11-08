Following CDISC SDTM standards, use the provided AE and CM datasets to create a
RELREC dataset with one record per relationship participation.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID

The link between a medication and an adverse event is collected on the
medication form only: CMAENO1 and CMAENO2 hold up to two adverse event
numbers, each naming the AE record of the same subject whose AESPID equals
it. Each event number on a CM record gives one relationship of two rows: a
row for the CM record, with RDOMAIN "CM", IDVAR "CMSEQ" and IDVARVAL its
CMSEQ, and a row for the AE record it names, with RDOMAIN "AE", IDVAR
"AESEQ" and IDVARVAL that AE record's AESEQ, not the event number. IDVARVAL
is the sequence number as text. A medication given for two events takes
part in two relationships, and an event treated by two medications likewise.
A CM record with no event number, and an AE record no medication names, give
no row. RELTYPE has no value on any row, because each row points at one
record rather than a whole dataset. RELID numbers the relationships within
each subject 1, 2, 3 and so on, ordered by the AESEQ of the event and then
the CMSEQ of the medication; the two rows of one relationship share it.

Read the source datasets from /app/input and save the completed dataset as
/app/output/relrec.csv.
