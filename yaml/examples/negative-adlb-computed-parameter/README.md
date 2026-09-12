# ADaM ADLB: reject a parameter computed from the dataset being built

This example uses collected liver-function results to attempt one row per
subject, collection date, and analysis parameter:

- `PARAMCD` and `PARAM` name the parameter. Each transaminase collected
  becomes one, and the ratio between the two becomes one more;
- `ADT` is the date the sample was collected;
- `AVAL` is the analysis value: the result as collected on a transaminase
  record, and the aspartate result over the alanine result on the ratio
  record. The ratio is empty when the day carries no usable alanine result to
  divide by, and when no aspartate result was measured that day, whether its
  record is absent or its value was never filled in.

The ratio is read from other rows of the dataset it belongs to, and no value
can be assembled from itself: reaching those rows reaches the analysis value
being defined. The run must therefore fail and no artifact is accepted. The
expected output records the intended result.

## How to fix

First decide which run owns the ratio. A parameter whose value comes from
other parameters belongs to a run that reads them as data, so build the two
transaminase parameters first and take the ratio in a second specification
that declares the completed dataset as one of its sources:

```yaml
datasets:
  ADLBIN: adlb.csv
```

The transaminase results are then ordinary records with keys, and the ratio is
an ordinary lookup of the aspartate result on the alanine record's date.

Do not reach the other parameter by counting rows instead. Which row sits one
position away depends on how the records happen to be ordered, so a sample
that answers correctly today stops doing so as soon as a third parameter is
added.
