# ADaM ADEX: reject a dose recorded with its unit

This example uses a subject-treatment inventory with its component exposure
records to attempt one record per subject and treatment:

- `DOSECUM` is the total dose administered across the exposure records.

The doses are numbers, and one was entered with its unit beside it. Reading the
digits and discarding the rest would accept a value the study did not record as
a number, and treating it as uncollected would remove an administration that
happened, so the run must fail and no artifact is accepted. A source that
carries such text is read as text, and the specification then says what to make
of it where a reader can see the answer.
