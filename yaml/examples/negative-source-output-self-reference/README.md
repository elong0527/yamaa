# ADaM ADLB: reject a parameter that reads the dataset it is part of

This example uses collected liver-function results, together with a file of
analysis values for the same dataset, to attempt one row per subject,
collection date, and analysis parameter:

- `PARAMCD` and `PARAM` name the parameter. Each transaminase collected
  becomes one, and the ratio between the two becomes one more;
- `ADT` is the date the sample was collected;
- `AVAL` is the analysis value: the result as collected on a transaminase
  record, and the aspartate result over the alanine result on the ratio
  record. The ratio is empty when either result is absent from the day.

The values the ratio reaches for are named as though they came from somewhere
else, but they are the values this run is producing, under the same name.
Nothing can distinguish the record that a run is writing from the record it is
reading back, so the run must fail before any data is read and no artifact is
accepted.

## How to fix

First decide whether the aspartate values the ratio reads are already final.
If they are, they belong to a finished dataset that this run merely reads, so
give that source a name of its own and keep the name of the dataset being
built for the dataset being built:

```yaml
datasets:
  LB: input/lb.csv
  ADLBIN: input/adlb.csv
```

The lookup then reads a completed dataset by its own name, which is an
ordinary source like any other.

If the aspartate values are not final, they are being produced by this run,
and renaming the source does not change that: it reads whatever the previous
run left behind, which is stale exactly when the two runs disagree. Split the
work instead, so the transaminase parameters are complete and written before
the run that reads them starts.
