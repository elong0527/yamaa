# What the Run Read, Built and Checked

**Goal:** report what a run did in one table: the run and its outcome, the
input it read, the rows it built, each output column, and every check it
evaluated, held or violated, in the order the run evaluated it.

**Input:** subject-level records with an `AGE` each; one age (214) falls
outside the expected 18-to-100 range.

**Checks:**

- `not_missing` on `AGE`: held, every age is present.
- `range` on `AGE`: violated by the age outside 18-to-100; it is a
  warning, so the run still succeeds and the run report lists the
  offending subject.
- `row_count`: held, the run produced at least one row.

The run report (`adsl-checks.csv`) has one row per item (`KIND`): the
`run` with its outcome (`OUTCOME`) and output row count, the `input` with
the records and bytes read, the `rows` built from the records read, one
`column` row per output column with its missing values and longest text,
and one `check` row per check. Every row names the dataset (`ARTIFACT`),
its place in the spec (`SPEC_PATH`) and its subject (`NAME`). A check row
also carries its optional label (`ID`), the column it covers (`TARGET`),
the rule it applies (`RULE`), its `SEVERITY`, and how many units failed out
of how many it examined (`COUNT`, `TOTAL`): rows for a row-by-row check
and the whole dataset as one unit for `row_count`. A held check leaves
`CONDITION` and `KEYS` empty and has empty details (`{}`).

**Note:** the report says what was read, built and checked, and for a
violated check what broke and for whom: its `KEYS` list every offending
subject, so it can be investigated without stopping the run, and its
`DETAILS` hold the remaining facts.
