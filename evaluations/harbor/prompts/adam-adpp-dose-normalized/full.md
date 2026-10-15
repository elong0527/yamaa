Following CDISC ADaM standards, use the provided PP and EX datasets to
create an ADPP dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL, AVALU

Keep every PP record, with PARAMCD from PPTESTCD, PARAM from PPTEST, AVAL
from PPSTRESN and AVALU from PPSTRESU. Add one dose-normalized record for
each maximum concentration and area under the curve record of a profile:
- "CMAXD" with PARAM "Max Conc Norm by Dose" for CMAX;
- "AUCLSTD" with PARAM "AUC to Last Nonzero Conc Norm by Dose" for AUCLST;
- "AUCIFOD" with PARAM "AUC Infinity Obs Norm by Dose" for AUCIFO.
PARAM holds no unit. The AVAL of an added record is the parameter's value
divided by the dose, in mg, of the administration that starts at the
profile's reference date and time: the EX record whose EXSTDTC equals
PPRFTDTC. That is the amount of this one administration, also on a day
with two administrations, and a dose recorded in ug counts as a thousandth
of a mg. AVALU is the parameter's unit followed by "/mg" ("ng/mL/mg",
"h*ng/mL/mg"). AVAL is not rounded. No dose-normalized record is added for
any other parameter, for a parameter that has no value, or for a profile
whose reference time has no dosing record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
