# Pair Each Adverse Event with the Doses of the Medications Given for It

**Goal:** give one row for each adverse event and each medication of the same
subject given for its term: their sequence numbers (`AESEQ`, `CMSEQ`), the
medication's name (`CMTRT`) and its dose as a whole number (`CMDOSE`); and a
run report that counts the doses left empty.

**Input:** one `spec.yaml` over adverse events, each with its sequence number
and reported term, and concomitant medications, each with its sequence
number, name, indication and dose as collected text; one dose is written in
words.

**Variables:**

- `CMTRT`: the name of a medication of the same subject whose indication is
  the event's term.
- `CMDOSE`: the medication's dose as a whole number; empty when the collected
  dose is not a number.

The run report (`aecm-report.csv`) has one row per item the run handled,
each naming the dataset (`ARTIFACT`), the kind of item (`KIND`), where the
spec states it (`SPEC_PATH`) and the item's name (`NAME`): the run and its
outcome (`OUTCOME`) with the rows it built (`COUNT`); the two collected
datasets, each with the records read and its file facts (`DETAILS`); the
pairs that built the rows, out of all pairs (`TOTAL`); then the doses left
empty because they are not numbers, counted once for each pair that holds
one. That count gives the rule its empty dose stands in for (`RULE`) and no
output column (`TARGET`), since each pair's dose is read before any row is
built. One row per output column follows, with its missing values and
longest text. Nothing is checked, so no row has a label (`ID`), a
`SEVERITY`, a `CONDITION` or `KEYS`.

**Note:** a subject who reports the same term twice pairs each of its
medications with both events, so a dose written in words is left empty in
each of those rows, and the run report counts it once for each: twice here.
