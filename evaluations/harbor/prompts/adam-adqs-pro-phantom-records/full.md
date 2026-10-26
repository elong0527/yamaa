Following CDISC ADaM standards, use the provided ADSL, QS, and SCHEDULE
datasets to create an ADQS dataset with one record per subject per parameter
per planned visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARCAT2, PARAMCD, PARAM, AVISIT, AVISITN, AVAL, DTYPE,
QSSTAT, QSREASND, AREASND, PROEX1FL, PROEX2FL, PROSCMFL

Every ADSL subject, in the safety population or not, has a record for each
SCHEDULE parameter (items FAT01 and FAT02, total score FATTOT) at each
planned visit. PARCAT2 ("ITEM" or "TOTAL SCORE"), PARAMCD and PARAM are the
planned parameter's, and AVISIT and AVISITN its VISIT and VISITNUM, from
SCHEDULE. An item is collected at a visit when QS holds a record of the
subject with that VISITNUM and with QSTESTCD equal to the item's PARAMCD,
answered or not done. A phantom record is an item record not collected, or a
FATTOT record at a visit where neither item was collected.

- AVAL is the item's QSSTRESN; on FATTOT it is the sum of the FAT01 and FAT02
  responses at the visit, with no value unless both were answered.
- DTYPE is "PHANTOM" on a phantom record and has no value otherwise.
- QSSTAT and QSREASND are the collected item's, with no value on a phantom
  record and on FATTOT.
- AREASND has no value when AVAL has a value. Otherwise, on a phantom record
  it is "NOT TREATED" for a subject whose SAFFL is "N", else the reason
  treatment ended (DCTREAS) at a visit planned after the end of treatment,
  else no value. On a collected item it is its QSREASND; on a FATTOT record
  that is not phantom, the QSREASND its items carry, or "NOT CALCULABLE" when
  neither carries one.
- PROEX1FL is "Y" for every subject at each visit not planned after death.
- PROEX2FL is "Y" for a subject whose SAFFL is "Y" at a visit planned after
  neither the end of treatment nor death.
- PROSCMFL is "Y" when AVAL has a value.
- PROEX1FL, PROEX2FL and PROSCMFL have no value where they are not "Y".

A visit is planned after an event when its VISITDY is later than the event's
day, both counted from RANDDT as day 1, with EOTDT the end of treatment and
DTHDT the death. No visit is planned after an event with no date, and a visit
planned on the event's day is still expected. Items collected at a visit
planned after the end of treatment are reported with their responses.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
