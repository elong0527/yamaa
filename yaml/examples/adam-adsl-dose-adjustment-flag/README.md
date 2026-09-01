# ADaM ADSL: derive a dose adjustment flag from multiple sources

This example uses ADSL, EX, EC, and FA data to return one record per subject:

- `DOSADJFL` is `Y` if the subject has a dose adjustment reported in `EX`,
  `EC`, or `FA`. If the subject appears in at least one of these datasets
  but no dose adjustment is reported, the value is `N`. If the subject is
  absent from all sources, the value is missing.

A qualifying record takes precedence over non-qualifying or missing records.
Matches use both study and subject identifiers, and source records without a
matching subject do not create output rows.
