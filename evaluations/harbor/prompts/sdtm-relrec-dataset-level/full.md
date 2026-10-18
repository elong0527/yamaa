Following CDISC SDTM standards, use the provided AE, CM, TR, and TU datasets to
create a RELREC dataset with one record per linked record or related dataset.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID

The TU dataset gives one dataset-level row and the TR dataset gives one
dataset-level row; the two rows relate the datasets as a whole. RDOMAIN
is "TU" on the TU row, "TR" on the TR row, "AE" on a row for an adverse
event record, and "CM" on a row for a concomitant medication record.
USUBJID has no value on a dataset-level row and names the subject on a
record-level row. IDVAR is "TULNKID" on the TU row and "TRLNKID" on the
TR row. IDVAR is "AESEQ" on an adverse event row and "CMSEQ" on a
concomitant medication row. IDVARVAL has no value on a dataset-level
row and is the sequence number of the related record, as text, on a
record-level row. RELTYPE is "ONE" on the TU row and "MANY" on the TR
row, because one tumor identification relates to many tumor results.
RELTYPE has no value on a record-level row. The dataset-level rows
share RELID "1". Each AE record that carries a link identifier gives
one row, and each CM record that carries a link identifier gives one
row. An AE or CM record without a link identifier gives no row. On a
record-level row RELID is the record's own link identifier, so an AE
record and a CM record that share a link identifier are related to one
another, and records with different link identifiers are in different
relationships. Rows that share a RELID are related to one another,
whether they point at whole datasets or at single records.

Read the source datasets from /app/input and save the completed dataset as
/app/output/relrec.csv.
