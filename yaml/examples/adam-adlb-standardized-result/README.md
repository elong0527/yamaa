# ADaM ADLB: carry standardized results into analysis

This example uses one laboratory record to produce one analysis record:

- `PARAMCD` identifies the laboratory test being analyzed.
- `AVAL` carries the standardized numeric result.
- `AVALU` carries the standardized unit for the result.

Shared organization, compound, and study definitions supply the common data
contract while the analysis dataset gives `AVAL` its final label.
