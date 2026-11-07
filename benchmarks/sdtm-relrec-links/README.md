# Related Records

**Goal:** relate each medication to the adverse events it was given
for, carrying `IDVARVAL`, `RELTYPE`, and `RELID`.

**Input:** adverse event (AE) records with their assigned sequence
number (`AESEQ`) and the event number collected on the form
(`AESPID`), and concomitant medication (CM) records with their
assigned sequence number (`CMSEQ`) and up to two event numbers
entered on the medication form as the events it treated (`CMAENO1`,
`CMAENO2`). The link is collected on the medication form only.

**Variables:**

- `IDVARVAL` is the sequence number of the related record as text:
  `CMSEQ` on a medication row and, on an event row, the `AESEQ` of
  the same subject's adverse event whose collected event number
  matches, never the collected number itself.
- `RELTYPE` is blank throughout, because each row points at one
  record rather than a whole dataset.
- `RELID` numbers the subject's medication and event pairs 1, 2,
  3 and so on, in adverse event sequence order and then medication
  sequence order; the two rows of one pair share it.

**Note:** each event number on a medication record gives one
relationship of two rows, one for the medication and one for the
event, so a medication given for two events takes part in two
relationships, as does an event treated by two medications. Event
numbers match within the same subject only. A medication without an
event number, and an event no medication names, contribute no row.
An event number that names no adverse event of that subject stops
the run, and so does a medication repeating one event number in both
fields, which would state the same relationship twice.
