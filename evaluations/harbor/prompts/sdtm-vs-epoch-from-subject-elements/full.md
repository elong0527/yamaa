Following CDISC SDTM standards, use the provided SE and VS_RAW datasets to
create a VS dataset with one record per vital signs measurement.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, EPOCH

DOMAIN is "VS". VSDTC is the collection date-time as collected, and has
no value when no date was collected. EPOCH is the trial period of the
subject's element in progress at the collection date-time: the element
whose start is on or before the collection and whose end is on or after
it. A collection that gives a date but no time reads as the start of
that day, so it falls in the element in progress at the start of that
day. A reading exactly at a shared element boundary belongs to the later
element. EPOCH has no value when the reading has no date or only a
partial one, or when the collection date-time falls outside every
element of the subject, such as before the first element starts.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
