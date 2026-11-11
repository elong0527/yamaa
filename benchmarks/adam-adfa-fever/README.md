# Fever Occurrence

**Goal:** derive `PARAMCD`, `PARAM`, `AVAL`, `AVALC`, `ADT`, `SRCDOM`,
`SRCVAR`, and `SRCSEQ` for each qualifying temperature record in an
ADaM analysis dataset (ADFA).

**Input:** Study Data Tabulation Model (SDTM) Vital Signs (VS)
records, one per collected result, carrying the test code, the
category, the result and unit as collected, the result in the
standard unit, the completion status, and the date and time of the
assessment.

**Variables:**

- `PARAMCD` and `PARAM` are always `FEVER` and `Fever Occurrence`.
- `AVALC` is `Y` for a temperature of 38 degrees Celsius or higher
  and `N` for a lower one, read from the result in the standard unit,
  Celsius (`C`), so a temperature collected in Fahrenheit is judged
  after its conversion; blank when the result is missing.
- `AVAL` is 1 when the occurrence flag is `Y`, 0 when it is `N`, and
  blank when the flag is blank.
- `ASEQ` numbers the subject's analysis records 1, 2, 3 ... in
  assessment-date order, the collected sequence breaking ties.
- `ADT` is the calendar date of the assessment.
- `SRCDOM`, `SRCVAR`, and `SRCSEQ` point back to the source result:
  always `VS`, `VSSTRESN`, and the collected sequence number.

**Note:** only temperature records in the reactogenicity category
qualify; other vital signs records give no row.
