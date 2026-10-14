Following CDISC ADaM standards, use the provided PP and PC datasets to
create an ADPP dataset with one record per PK parameter per subject per
period.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISITNUM, VISIT, PARAMCD, PARAM, AVAL, PRECONC,
PREPCMAX, ANL01FL

PARAMCD and PARAM are the PP test code and test name, and AVAL is the
numeric result in standard units.

A parameter belongs to the dosing period whose reference dose time it
carries (PPRFTDTC). The period's pre-dose sample is the PC record
planned at time point PREDOSE whose reference dose time (PCRFTDTC) is
the same; no other sample stands in for it. PRECONC is the
concentration of that sample, on every parameter record of the period.
A pre-dose result below the lower limit of quantitation, given in
PCSTRESC as "<" followed by the limit, counts as 0. PRECONC has no
value when the period has no pre-dose sample or the sample was not done.

PREPCMAX is PRECONC as a percentage of the same subject's Cmax (PPTESTCD
CMAX) in the same period, not rounded, on every parameter record of the
period. It has no value when PRECONC has none.

ANL01FL marks the records that enter the bioequivalence analysis. It
has no value on every parameter record of a period whose PREPCMAX is
greater than 5, and is Y on every other record, including those of a
period whose PREPCMAX has no value; exactly 5 stays in. Every period is
checked, the first one included, and only the affected period is left
out: the subject's other period keeps Y. All records stay in the output.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
