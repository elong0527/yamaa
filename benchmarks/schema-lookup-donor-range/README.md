# Place Each Dose in Its Period to Find the First Dose of Period 2

**Goal:** give each subject the date of their first dose in treatment period
2 (`TR02SDT`).

**Input:** the subjects, their dosing records with a start date and the
visit each dose was given at, the visits with their start dates, and each
subject's treatment periods, each from a first to a last day.

**Variables:**

- `TR02SDT` is the start date of the subject's earliest dose whose period
  is period 2, and blank for a subject with no dose in period 2.

**Note:** each dose is placed in the period whose first and last days, both
inclusive, hold that dose's own date, so a dose on the first day of a period
belongs to that period.

## Specification variants

- `spec_date.yaml` places each dose by its own start date.
- `spec_visit.yaml` places each dose by the start date of the visit it was
  given at, looked up for that dose; doses fall on their visit's date, so
  both build the same dataset.
