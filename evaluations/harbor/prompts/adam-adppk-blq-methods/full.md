Following CDISC ADaM standards, use the provided EX and PC datasets to create
an ADPPK dataset with one record per subject per administration (a dose
record) or sample that was done (a concentration record).

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADTM, EVID, AMT, ALLOQ, CENS, DV, MDV, DVM6, MDVM6

A PC sample with PCSTAT "NOT DONE" was not done and gives no record. A
quantified result has its number in PCSTRESN; a result below the limit of
quantification has none, and its PCSTRESC starts with "<".

- ASEQ numbers the subject's records from 1 in the order of ADTM, with a
  sample before a dose at the same date/time.
- ADTM is EXSTDTC on a dose record and PCDTC on a concentration record,
  written as YYYY-MM-DDThh:mm:ss with its seconds.
- EVID is 1 on a dose record and 0 on a concentration record.
- AMT is EXDOSE on a dose record and has no value on a concentration record.
- ALLOQ is PCLLOQ on a concentration record and has no value on a dose
  record.
- CENS is 1 for a result below the limit, and 0 for a quantified result and
  on a dose record.
- DV is the quantified result, or ALLOQ for a result below the limit; it has
  no value on a dose record.
- MDV is 1 when DV has no value and 0 otherwise.
- DVM6 is the quantified result. For a result below the limit it is half of
  ALLOQ, not rounded, when the subject's sample before it was quantified;
  it has no value when that sample was also below the limit or when the
  subject has no sample before it. DVM6 has no value on a dose record.
- MDVM6 is 1 when DVM6 has no value and 0 otherwise.

The sample before a result is the subject's latest earlier concentration
record, whatever doses were given in between. A sample that was not done
does not separate a result below the limit from the quantified result
before it.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adppk.csv.
