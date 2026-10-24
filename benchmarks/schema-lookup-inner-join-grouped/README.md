# Count the Medications Given for Each Treated Adverse Event

**Goal:** give one row for each adverse event that at least one medication
of the same subject was given for, with the number of such medications
(`CMCOUNT`).

**Input:** one `spec.yaml` over adverse events, each with its sequence number
and reported term, and concomitant medications, each with its sequence
number, name and indication, the indication written as the term of the event
it was given for.

**Variables:**

- `AETERM`: the reported term of the adverse event.
- `CMCOUNT`: how many of the subject's medications name the event's term as
  their indication.

**Note:** an event pairs with each medication of the same subject whose
indication is its term, and the event's pairs build its one row. An event no
medication of the same subject was given for has no row, even when another
subject took a medication for the same term.
