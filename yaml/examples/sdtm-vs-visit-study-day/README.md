# SDTM VS: attach visit metadata and study day to a result

This example uses collected vital signs with DM and the trial-visits table and
a `yamaa` specification to derive one record per collected result:

- `VSTESTCD`, `VSORRES`, `VSDTC`, and `VISIT` are the test, the result, the
  date, and the visit label as collected;
- `VISITNUM` and `EPOCH` are the visit's number and its period in the trial
  design, looked up by the collected visit label. A visit the design does not
  name, such as an unscheduled one, has neither;
- `VSDY` is the study day of the result, counted from the subject's reference
  start date. That date is day 1 and there is no day zero, so a result
  collected before it counts back from -1. A result with no date, and one
  belonging to a subject with no reference date, has no study day.

The reference date itself is not part of a VS record, so it is used to derive
the study day and then dropped.
