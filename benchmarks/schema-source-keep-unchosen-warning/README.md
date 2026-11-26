# Report a First-Value Choice That Chose Nothing

**Goal:** derive each vital signs result (`AVAL`) and what was meant as the
subject's first result (`BASE`), and report that the choice of a first
result never had two results to choose between.

**Input:** one `spec.yaml` over one dataset. `VS` carries one row per vital
signs result: subject, sequence number and the numeric result.

**Variables:**

- `USUBJID` and `VSSEQ` are the result's subject and sequence number; each
  output row is one result.
- `AVAL` is the result.
- `BASE` was written to keep the subject's result with the lowest sequence
  number. It chooses only among the results that build one output row, and
  each output row is built from one result, so it holds the row's own
  result, the same as `AVAL`.

The run report (`advs-checks.csv`) has one row per item the run handled, each
naming the dataset (`ARTIFACT`), the kind of item (`KIND`), where the spec
states it (`SPEC_PATH`) and the item's name (`NAME`): the run and its outcome
(`OUTCOME`), the input dataset read, the rows built, the count of rows where
the first result chose between two results, which names the column it feeds
(`TARGET`), and one row per output column. One warning follows, at the choice
of the first result. It has no label (`ID`) or column (`TARGET`); it names
the rule it reports (`RULE`), its `SEVERITY` and what it found
(`CONDITION`), counts the choice as one unit (`COUNT`, `TOTAL`, `KEYS`), and
its `DETAILS` name the result it reads.

**Note:** the warning changes no value and fails nothing. A first result per
subject is chosen over the subject's results apart from the rows, such as a
lookup over the vital signs keyed by subject, and gives no warning.
