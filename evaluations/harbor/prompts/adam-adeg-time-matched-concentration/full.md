Following CDISC ADaM standards, use the provided ADEG_RAW and ADPC datasets to
create an ADEG dataset with one record per subject, parameter, analysis
period, nominal day within the period, and nominal time point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRTA, PARAMCD, PARAM, APERIOD, APERDAY, ATPT, ATPTN, AVAL,
AVALU, ABLFL, BASE, CHG, BASETYPE, ECGPCFL, PKCONC, CQTANLFL

ECGPCFL is Y when the concentrations hold a sample of the parent drug
DRGX for the same subject, period, period day, and nominal time point; it
has no value otherwise. A sample whose result is below the lower limit of
quantification still matches. A sample at another time point, even the
nearest one, never matches, and neither does a metabolite result.

PKCONC is the matching sample's concentration in ng/mL, as recorded and
not rounded, on records of any treatment. PKCONC has no value when the
matching sample's result is below the lower limit of quantification or
when no sample matches. On placebo records, where TRTA is "Placebo",
PKCONC is 0 whenever no quantified result exists, on the baseline day as
on any other day. Whether a record is a placebo record follows its own
period's treatment.

CQTANLFL is Y when the record has both a change from baseline and a
concentration; it has no value otherwise. A PK sample with no ECG record
at its time point adds no record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
