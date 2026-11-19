# Weight, Creatinine and Creatinine Clearance at the Time of Each Record

**Goal:** give every dose record and every pharmacokinetic (PK)
concentration record of a population PK dataset the body weight (`WT`)
and the serum creatinine (`CREAT`) that applied on its date, and the
creatinine clearance (`CRCL`) those two give.

**Input:** exposure (EX) records, one per administration, with the dose
(`EXDOSE`) and its date/time (`EXSTDTC`); PK concentration (PC) records
with the collection date/time (`PCDTC`); vital signs (VS) and laboratory
(LB) records, one per subject, test and date (`VSDTC`, `LBDTC`), with the
numeric result (`VSSTRESN` in kg for the test `WEIGHT`, `LBSTRESN` in
umol/L for the test `CREAT`); and the subject-level dataset (ADSL) with
the age in years (`AGE`) and the sex (`SEX`). An administration gives a
dose record and a sample gives a concentration record.

**Variables:**

- `ASEQ`: the record's number within the subject, in the order of `ADTM`,
  a sample before a dose at the same date/time.
- `ADTM`: the date/time of the administration or of the sample.
- `EVID`: 1 for a dose record and 0 for a concentration record.
- `WT`: the subject's latest weight result dated on or before the date of
  `ADTM`; empty when there is none.
- `CREAT`: the subject's latest creatinine result dated on or before the
  date of `ADTM`; empty when there is none.
- `CRCL`: the Cockcroft-Gault creatinine clearance in mL/min, not
  rounded: (140 - `AGE`) x `WT` x 1.23 / `CREAT` for a male subject, with
  1.04 in place of 1.23 for a female subject; empty when `WT` or `CREAT`
  is empty.

**Note:** a result counts from its date onward, whatever the time of day
of the record, and a result dated after the record is never used. A
record of the test with no result, such as a weight not measured at a
visit, is passed over, so the earlier result stays in force.
