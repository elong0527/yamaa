# Start Dates Read From Each Row's Own Record

**Goal:** build one timeline row per subject, adverse event and
concomitant medication, and complete the start date (`STDT`) of every
event and medication row from the record that built it.

**Input:** one `spec.yaml` over the subjects (`input/dm.csv`: domain,
subject and reference start date), their adverse events
(`input/ae.csv`: domain, subject, sequence, reported term and start
date) and their concomitant medications (`input/cm.csv`: domain,
subject, sequence, medication name and start date).

**Variables:**

- `SEQ`: the sequence number of the record behind the row, and `1` on the
  subject's own row.
- `TERM`: the reported adverse event term or medication name, and
  `REFERENCE START` on the subject's own row.
- `STDT`: the start date collected on the record behind the row: the
  adverse event's or the medication's own start date, and the
  subject's reference start date on the subject's row; blank when that
  record has no start date.

**Note:** the start date is stated once for the event and medication
rows, naming the start dates of both. Each row reads the date of its
own record. The other file is matched on the subject and the domain,
so a medication row never takes an adverse event's date, and an event
without a start date stays blank even when the subject's medication
has one.
