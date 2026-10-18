Following CDISC ADaM standards, use the provided AE and QUERIES datasets to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AEDECOD, SMQ01NAM, SMQ01CD, SMQ01SC, SMQ02NAM,
SMQ02CD, SMQ02SC, CQ01NAM

The QUERIES dataset is a query dictionary that lists coded terms under
query groupings. Membership of an event in a grouping is decided by its
AEDECOD: an event belongs to a grouping when the dictionary lists its
coded term under that grouping, and two events with the same coded term
always show the same grouping entries. A grouping entry in the dictionary
gives the grouping's name, and for a standardized grouping also its code
and whether the term's membership in it is "BROAD" or "NARROW" scope.
SMQ01NAM, SMQ01CD, and SMQ01SC hold the name, code, and scope of the
first standardized query grouping the event's coded term belongs to,
and have no value when the coded term is not in that grouping. SMQ02NAM,
SMQ02CD, and SMQ02SC hold the name, code, and scope of the second
standardized query grouping in the same way. CQ01NAM holds the name of
the sponsor-defined customized query grouping the event's coded term
belongs to, and has no value when the coded term is not listed under it;
no code or scope applies to that grouping. An event can belong to a
standardized grouping and to the sponsor grouping at the same time, each
shown in its own columns. An event with no AEDECOD belongs to no
grouping, so all seven grouping columns have no value for it.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
