# Required Values on Selected Rows and Within a Share

**Goal:** require a value only on the rows a condition selects, and allow a
share of the rows to lack one: a share at its limit, a deliberately violated
warning over it, and a condition that selects no row.

**Input:** one `spec.yaml` over adverse events, one row per subject and
event, each with a reported and a coded term (`AETERM`, `AEDECOD`), a
seriousness flag (`AESER`), a death flag (`AESDTH`) and start and end dates
(`AESTDTC`, `AEENDTC`).

**Checks:**

- A coded term is required on every event with a reported term. Events
  without a reported term are not checked.
- A death flag is required on every serious event. Events that are not
  serious, or whose seriousness is unknown, are not checked.
- At most a quarter of the events may lack a start date, and the boundary
  counts as held.
- At most a quarter of the events with a reported term may lack an end
  date. Exceeding that share produces a warning: the run still succeeds
  and the run report lists the affected events.
- An end date is required on every event that resulted in death. If no
  event is selected, the share is zero and the check holds.

The run report (`ae-checks.csv`) has one row per item the run handled, each
naming the dataset (`ARTIFACT`), the kind of item (`KIND`), where the spec
states it (`SPEC_PATH`) and the item's name (`NAME`, identifying an input,
row driver, handler, column, or check): the run and its outcome (`OUTCOME`),
the events read and built, each output column with its missing
values and longest text, then one row per check in column order. A check row
carries its label (`ID`), the column it covers (`TARGET`), the rule it
applies (`RULE`), its `SEVERITY`, and how many events failed out of every
event of the dataset (`COUNT`, `TOTAL`). A held check leaves `CONDITION`
and `KEYS` empty, reports zero failures and has empty details (`{}`). The
violated warning names its condition, lists the affected events in `KEYS`,
and reports in `DETAILS` its column and the number of events it checked.

**Note:** a check with a condition reads only the rows the condition selects;
a row whose condition is false or unknown passes whatever it holds. A share
is the selected rows lacking the value over the selected rows, so rows
outside the condition count in neither, and a condition that selects no row
has a share of zero.
