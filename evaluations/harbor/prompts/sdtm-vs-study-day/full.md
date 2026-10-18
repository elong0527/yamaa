Following CDISC SDTM standards, use the provided DM, SE, TV, and VS_RAW
datasets to create a VS dataset with one record per collected
vital-signs result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, VISIT, VISITNUM,
EPOCH, VSDY

DOMAIN is "VS". VSSEQ is the sequence number as collected. VISITNUM is
the planned visit number in TV for the record's visit label. An
unplanned visit label that names no planned visit in TV, such as
"UNSCHEDULED", takes the sponsor-assigned number: the latest planned
visit number collected for the subject before the collection date,
plus 0.01, so an unscheduled visit after visit 2 reads 2.01. EPOCH is
the epoch of the subject's element in SE whose start and end dates,
both ends included, contain the collection date: "SCREENING",
"TREATMENT", or "FOLLOW-UP". A day shared by two elements belongs to
the later element. EPOCH has no value when the record has no
collection date or when no element contains the collection date.
VSDY is the study day of the collection date, counted from the
subject's reference start date in DM. VSDY has no value when the
record has no collection date or the subject has no reference start
date in DM. The visit number follows the visit label while the epoch
follows the collection date, so an unscheduled visit still falls in
an epoch, and a planned visit with no collection date has a visit
number but no epoch.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
