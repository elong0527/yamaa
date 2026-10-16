Following CDISC ADaM standards, use the provided PC dataset to create an
ADPC dataset with one record per sample.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PCSEQ, PARAMCD, ATPTREF, ATPT, ATPTN, AVAL, AVALU

A profile is the samples of one subject and analyte after one dose, in
planned time order. A quantified sample is a sample with a numeric
result. AVAL is the numeric result of a quantified sample, as recorded.

A result below the limit of quantification is reported as text starting
with "<" and has no numeric result. Its AVAL is 0 when it falls before
the first quantified sample of its profile or after the last one, and
has no value when it falls between them. In a profile with no quantified
sample, every result below the limit has AVAL 0.

A sample not done has no value, and it neither starts nor ends the
quantified span of its profile.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpc.csv.
