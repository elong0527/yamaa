Following CDISC ADaM standards, use the provided EX and PC datasets to create
an ADPPK dataset with one record per event of a subject: a dose
administration, or a PK concentration sample of one analyte.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ID, ASEQ, ADTM, EVID, CMT, PARAMCD, AMT, DV, MDV

The dataset holds the events as a population PK model reads them: a dose
record for each EX record and a concentration record for each PC record,
with these exceptions.
- An administration with a dose of zero is no event and gives no record.
- A sample that was not done (PCSTAT is "NOT DONE") gives no record.
- A subject with no administration of a dose above zero, such as a placebo
  subject, has no records at all, concentration records included.
- Every other PC record gives a concentration record, a sample drawn before
  the subject's first dose and a result below the limit of quantification
  included.

The columns hold:
- ID is the subject's number among the subjects in the output dataset,
  counted from 1 in the order of USUBJID, with no number skipped.
- ASEQ is the record's number within the subject, from 1 in the order of
  ADTM. At one date/time a concentration record comes before a dose record,
  and concentration records follow their CMT.
- ADTM is the date/time of the administration (EXSTDTC) or of the sample
  (PCDTC), written as YYYY-MM-DDThh:mm:ss with the seconds always shown.
- EVID is 1 for a dose record and 0 for a concentration record.
- CMT is 1 for a dose record, and for a concentration record 2 when
  PCTESTCD is "DRGX", the parent drug, and 3 when it is "MET1", its
  metabolite.
- PARAMCD is "DOSE" for a dose record and PCTESTCD for a concentration
  record.
- AMT is the dose given (EXDOSE) and has no value on a concentration record.
- DV is the concentration (PCSTRESN), not rounded, and has no value on a
  dose record or for a result below the limit of quantification.
- MDV is 1 when DV has no value, on every dose record too, and 0 otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adppk.csv.
