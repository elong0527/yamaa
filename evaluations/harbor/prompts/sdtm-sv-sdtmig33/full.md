Following CDISC SDTM standards, use the provided VISITS dataset to create an SV
dataset with one record per subject per visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VISITNUM, VISIT, SVSTDTC, SVENDTC, SVUPDES

Use SDTMIG 3.3. DOMAIN is "SV". Include planned and unplanned visits,
with fractional VISITNUM values identifying unplanned visits between scheduled
visits. Visits spanning more than one day have their actual start and end
dates. SVUPDES explains an unplanned visit and has no value for planned
visits.

Read the source datasets from /app/input and save the completed dataset as
/app/output/sv.csv.
