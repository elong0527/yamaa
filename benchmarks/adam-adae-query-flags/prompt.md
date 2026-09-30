Following CDISC ADaM standards, use the provided AE and QUERY datasets to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AEDECOD, SMQ01NAM, SMQ01CD, SMQ01SC,
SMQ02NAM, SMQ02CD, SMQ02SC, CQ01NAM

Show each coded term's entries from the query dictionary in their own
slots. Each standardized slot shows its grouping name, dictionary code,
and scope together: BROAD or NARROW. The sponsor slot shows the
grouping name only. An event can sit in a standardized grouping and in
the sponsor grouping at the same time, each shown in its own place.
Two events with the same coded term always show the same grouping
entries. An event still awaiting coding, with no AEDECOD, belongs to
no grouping, so the grouping columns have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
