# Reject One Fragment Added Twice

**Goal:** add the analysis duration (`ADURN`) to an adverse event dataset
from a reusable duration file, on top of the adverse event template.

**Input:** collected adverse events, one record per event; the template in
`layers/ae.yaml`; and the duration file in `layers/duration.yaml`.

**Variables:**

- `ADURN` would be the number of days from the analysis start date to the
  analysis end date, counting both days.

**Note:** the template already adds the duration file, and the entry adds it
again on top of the template. A file contributes to a dataset at most once,
because a second copy would apply its definitions again after everything
between the two copies, so the run is rejected before any data is read.

## How to fix

Add the duration file in one place. Keep it in the template when every
dataset built on the template needs the duration, and remove it from the
entry's `include` list:

```yaml
yamaa: "1.0"
type: spec
parent: layers/ae.yaml
domain: ADAE
```

Otherwise remove it from the template and list it only in the entries that
need it.
