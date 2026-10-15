Following CDISC ADaM standards, use the provided ADEG_RAW and ADPC datasets
to create an ADEG dataset with one record per subject per parameter per
analysis period per nominal period day per nominal time point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRTA, PARAMCD, PARAM, APERIOD, APERDAY, ATPT, ATPTN,
AVAL, AVALU, ABLFL, BASE, CHG, BASETYPE, ECGPCFL, PKCONC, CQTANLFL

Each ADEG_RAW record gives one output record; an ADPC sample without an
ECG record adds none. An ECG record's sample is the ADPC record of the
parent drug (PARAMCD "DRGX") with the same subject, analysis period
(APERIOD), nominal period day (APERDAY) and nominal time point (ATPTN).
The match is exact: a sample at another time point, even the nearest one,
and a metabolite record are never used.
- ECGPCFL is "Y" when the record has such a sample, whatever its result,
  also a result below the limit of quantification; otherwise it has no
  value.
- PKCONC is the sample's AVAL as recorded, not rounded, on records of any
  treatment. Nothing is imputed: PKCONC has no value for a result below
  the limit of quantification and when the record has no sample. The one
  exception is placebo: a record whose TRTA is "Placebo" has PKCONC 0
  whenever it has no quantified result, on the baseline day too.
- CQTANLFL is "Y" when the record has both a CHG and a PKCONC value;
  otherwise it has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
