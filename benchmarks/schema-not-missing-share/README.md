# Required Values on Selected Rows and Within a Share

**Goal:** require a value only on the rows a condition selects, and allow a
share of the rows to lack one: a share at its limit, a deliberately violated
warning over it, and a condition that selects no row.

**Input:** one `spec.yaml` over adverse events, one row per subject and
event, each with a reported and a coded term (`AETERM`, `AEDECOD`), a
seriousness flag (`AESER`), a death flag (`AESDTH`) and start and end dates
(`AESTDTC`, `AEENDTC`).

**Checks:**

- A coded term on every event with a reported term: the event without a
  reported term lacks a coded term too and is not checked, so the check
  holds.
- A death flag on every serious event: the events that are not serious
  lack one and are not checked, nor is the event whose seriousness is
  missing, for which the condition is unknown; the check holds.
- At most a quarter of the events lack a start date: exactly a quarter do,
  and the boundary counts as held.
- At most a quarter of the events with a reported term lack an end date:
  two of those seven do, more than a quarter. It is a warning, so the run
  still succeeds and the run report lists the two events.
- An end date on every event that resulted in death: none did, so no event
  is checked and the check holds.

The run report (`ae-checks.csv`) has one row per item the run handled, each
naming the dataset (`ARTIFACT`), the kind of item (`KIND`), where the spec
states it (`SPEC_PATH`) and its subject (`NAME`): the run and its outcome
(`OUTCOME`), the events read and built, each output column with its missing
values and longest text, then one row per check in column order. A check row
carries its label (`ID`), the column it covers (`TARGET`), the rule it
applies (`RULE`), its `SEVERITY`, and how many events failed out of every
event of the dataset (`COUNT`, `TOTAL`). A held check leaves `CONDITION`
and `KEYS` empty, reports zero failures and has empty details (`{}`). The
violated warning names its condition, lists the two events in `KEYS`, and
reports in `DETAILS` its column and the seven events it checked.

**Note:** a check with a condition reads only the rows the condition selects;
a row whose condition is false or unknown passes whatever it holds. A share
is the selected rows lacking the value over the selected rows, so rows
outside the condition count in neither, and a condition that selects no row
has a share of zero.
