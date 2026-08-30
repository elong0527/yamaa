# ADaM ADSL: select the first rescue medication

This example uses collected demographics with the medications taken alongside
study treatment to derive one record per subject:

- `RESCTRT` is the medication a subject first received as rescue, taken from
  the earliest one recorded as rescue and, where two share a start date, the
  one recorded first.

A subject whose medications include none given as rescue has no such
medication, and the value is empty. That is the same result a subject with no
recorded medications at all receives, because in both cases nothing was
selected rather than something being selected badly.
