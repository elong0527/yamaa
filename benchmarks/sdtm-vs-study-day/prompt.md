Following CDISC SDTM standards, use the provided VS_RAW, DM, TV, and SE
datasets to create a VS dataset with one record per collected vital-signs
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, VISIT, VISITNUM,
EPOCH, VSDY

VISITNUM is the planned visit number from the trial-visits table for the
collected visit label. An unplanned label such as UNSCHEDULED names no
planned visit, so it takes a sponsor-assigned number based on the latest
planned visit number collected before the collection date, and an
unscheduled visit after visit 2 reads 2.01. EPOCH is the subject's element
whose start and end dates, both ends included, contain the collection
date: SCREENING, TREATMENT, or FOLLOW-UP. A day shared by two elements
belongs to the later element. EPOCH has no value when there is no
collection date or no element contains it. VSDY is the study day of the
collection date relative to the subject's reference start date.

The visit number follows the visit label while the epoch follows the
collection date, so an unscheduled visit still falls in an epoch, and a
planned visit with no collection date has a visit number but no epoch.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
