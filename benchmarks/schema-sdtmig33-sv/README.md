# Describe actual subject visits with SDTMIG 3.3

Goal: Keep the actual start and end of each visit, including an unplanned
visit.

Input: Collected visit names, numbers, dates and reasons from a visit form.

Variables:

- `VISITNUM` and `VISIT` identify the visit; fractional visit numbers retain
  unplanned visits between scheduled ones.
- `SVSTDTC` and `SVENDTC` retain the visit dates, including visits spanning
  more than one day.
- `SVUPDES` explains an unplanned visit and is absent for planned visits.

Note: `project.yaml` declares SDTMIG 3.3. The data-definition document
records that guide and points to the SAS Transport version 5 dataset.
