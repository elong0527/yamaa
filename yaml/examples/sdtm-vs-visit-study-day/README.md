# SDTM VS: attach visit metadata and study day to a result

This example uses collected vital signs with DM, the trial-visits table, and a
study-defined epoch-range table to derive one record per collected result:

- `VSTESTCD`, `VSORRES`, `VSDTC`, and `VISIT` are the test, the result, the
  date, and the visit label as collected;
- `VISITNUM` is the visit's planned number, looked up by the collected visit
  label. A visit the design does not name, such as an unscheduled one, has no
  planned number;
- `EPOCH` is the trial period whose study-day range contains the result. An
  unscheduled result still has a period when its study day is known, while a
  result with no study day has none;
- `VSDY` is the study day of the result, counted from the subject's reference
  start date. That date is day 1 and there is no day zero, so a result
  collected before it counts back from -1. A result with no date, and one
  belonging to a subject with no reference date, has no study day.

The reference date itself is not part of a VS record, so it is used to derive
the study day and then dropped. This example assumes all subjects share epoch
transitions expressed relative to that reference date; a design whose epochs
follow arm-specific or actual subject element dates needs a correspondingly
keyed source. The epoch-range input is an example fixture, not a standard SDTM
trial-design domain.
