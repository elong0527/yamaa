Following CDISC ADaM standards, use the provided ADLB_RAW dataset to create an
ADLB dataset with one record per laboratory record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ASEQ, AVISIT, AVAL, ANRLO, ANRHI, ANRIND,
ABLFL, BASE, BNRIND, SHIFT1, R2BASE, CRIT1, CRIT1FL, CRIT2, CRIT2FL

- ANRIND is "LOW" when AVAL is below ANRLO, "HIGH" when AVAL is above
  ANRHI, and "NORMAL" when AVAL is between ANRLO and ANRHI, limits
  included, so a result exactly at a limit is "NORMAL".
- ANRIND has no value when AVAL, ANRLO, or ANRHI has no value.
- BASE repeats the baseline record's AVAL on every record of the subject
  and parameter, where the baseline record is the one with ABLFL "Y".
- BASE has no value when no record of the subject and parameter has
  ABLFL "Y".
- BNRIND repeats the baseline record's ANRIND on every record of the
  subject and parameter.
- BNRIND has no value when there is no baseline record, or when the
  baseline record's own ANRIND has no value.
- SHIFT1 is the baseline mark and the record's own mark joined by " to ",
  baseline first, so a record that stayed normal reads "NORMAL to NORMAL"
  and one that moved out of range reads "NORMAL to HIGH".
- SHIFT1 has no value when either ANRIND or BNRIND has no value.
- R2BASE is AVAL divided by BASE, so the baseline record itself reads 1
  when the ratio can be computed.
- R2BASE has no value when AVAL has no value, when there is no baseline,
  or when BASE is zero.
- R2BASE is not rounded.
- CRIT1 is "Result greater than 3 x ULN" when AVAL and ANRHI have values,
  and has no value when AVAL or ANRHI has no value.
- CRIT1FL is "Y" when AVAL is greater than three times ANRHI and "N" when
  it is not, and a result exactly at three times ANRHI does not meet the
  criterion.
- CRIT1FL has no value when AVAL or ANRHI has no value.
- CRIT2 is "Result less than LLN" when AVAL and ANRLO have values, and
  has no value when AVAL or ANRLO has no value.
- CRIT2 is assessed independently of CRIT1, so a record whose ANRHI has
  no value can still meet the low criterion.
- CRIT2FL is "Y" when AVAL is less than ANRLO and "N" when it is not, and
  a result exactly at ANRLO does not meet the criterion.
- CRIT2FL has no value when AVAL or ANRLO has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
