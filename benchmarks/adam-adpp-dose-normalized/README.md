# Add dose-normalized exposure parameters

**Goal:** keep every pharmacokinetic (PK) parameter record and add, for each
maximum concentration (`CMAX`) and area under the curve (`AUCLST`, `AUCIFO`)
record, its dose-normalized parameter (`CMAXD`, `AUCLSTD`, `AUCIFOD`): the
value divided by the dose, in milligrams, of the administration the profile
follows.

**Input:** PK parameters (PP), one record per parameter per concentration
profile, each profile identified by its reference date and time; exposure
(EX), one record per administration with its start date and time, dose and
dose unit.

**Variables:**

- `PARAMCD`: the PP parameter code, or `CMAXD`, `AUCLSTD` or `AUCIFOD` on an
  added record.
- `PARAM`: the PP parameter name, or the controlled name of the added
  parameter (`Max Conc Norm by Dose`, `AUC to Last Nonzero Conc Norm by
  Dose`, `AUC Infinity Obs Norm by Dose`), without a unit.
- `AVAL`: the PP result in standard units, empty when the parameter was not
  calculated; on an added record, that result divided by the dose of the
  one administration that starts at the profile's reference time, a dose in
  micrograms counting as a thousandth of a milligram. Not rounded.
- `AVALU`: the PP standard unit; on an added record, that unit per mg
  (`ng/mL/mg`, `h*ng/mL/mg`).

**Note:** a twice-daily day divides by one administration's dose, not the
day's total. No dose-normalized record is added for any other parameter
(time of maximum concentration, half-life), for a parameter that has no
value, or for a profile whose reference time has no dosing record or a zero
dose. Two dosing records with one start, a dose unit other than mg or ug, a
negative dose, or an input that already holds a dose-normalized code stops
the run.
