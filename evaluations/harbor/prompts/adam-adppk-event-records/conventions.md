Following CDISC ADaM standards, use the provided EX and PC datasets to create
an ADPPK dataset with one record per event of a subject: a dose
administration, or a PK concentration sample of one analyte.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ID, ASEQ, ADTM, EVID, CMT, PARAMCD, AMT, DV, MDV

ID is the subject's number among the subjects in the output dataset, counted
from 1 in the order of USUBJID, with no number skipped. ADTM is written as
YYYY-MM-DDThh:mm:ss with the seconds always shown. EVID is 1 for a dose
record and 0 for a concentration record. CMT is 1 for a dose record, and for
a concentration record 2 when PCTESTCD is "DRGX", the parent drug, and 3
when it is "MET1", its metabolite. PARAMCD is "DOSE" for a dose record and
PCTESTCD for a concentration record. DV is not rounded. MDV is 1 when DV has
no value and 0 otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adppk.csv.
