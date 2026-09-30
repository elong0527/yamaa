Following CDISC SDTM standards, use the provided VS_RAW and SE datasets to
create a VS dataset with one record per collected vital-signs result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, EPOCH

VSDTC is the collection date-time, carried over unchanged, and has no
value when no date was collected. EPOCH is the trial period of the element
in progress at the collection date-time: the element whose start is on or
before the collection and whose end is on or after it. It has no value
when the reading has no date or only a partial one, or when it falls
outside every element, such as before the first element starts.

The first-dose day shows why a date-time matters: a pre-dose reading on
that day belongs to screening and a post-dose reading to treatment. A
date-only collection reads as the start of that day. A reading exactly at
a shared element boundary belongs to the later element.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
