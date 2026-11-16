Following CDISC ADaM standards, use the provided PC dataset to create an ADPC
dataset with one record per sample.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PCSEQ, PARAMCD, ATPTREF, ATPT, ATPTN, AVAL, AVALU,
NCAXFL

PCSEQ is the sample's sequence number and PARAMCD its analyte, as in the
input. ATPTREF is the dose the sample follows, ATPT its planned time
point, and ATPTN that time point numbered in hours, as in the input. A
profile is the samples of one subject and analyte after one dose, in
planned time order. AVAL is the concentration in ng/mL. For a quantified
result, AVAL is the result as recorded. A result below the lower limit of
quantification is reported as text starting with "<", with no number,
and AVAL is 0 for every such result. A sample not done has no result,
and AVAL has no value. AVALU is "ng/mL". NCAXFL is "Y" for a result
below the limit that comes after the first quantified result of its
profile and before the last one, and has no value otherwise: for a
result below the limit before the first or after the last quantified
result, for every result of a profile with no quantified result, and for
quantified results and samples not done. A sample not done neither starts
nor ends the quantified span of its profile.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpc.csv.
