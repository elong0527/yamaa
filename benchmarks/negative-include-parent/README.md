# Reject a Fragment That Names Its Own Template

**Goal:** add the analysis duration (`ADURN`) to an adverse event dataset
from a reusable duration file, on top of the adverse event template.

**Input:** collected adverse events, one record per event; the template in
`layers/ae.yaml`; and the duration file in `layers/duration.yaml`, which the
entry adds to the template.

**Variables:**

- `ADURN` would be the number of days from the analysis start date to the
  analysis end date, counting both days.

**Note:** the duration file still names the template as its own parent, as
it did when every study file named exactly one file to build on. A file
added on top of a template must not name a template of its own, since it is
meant for whichever template adds it, so the run is rejected before any data
is read and no artifact is accepted.

## How to fix

Remove `parent` from the reusable file. The entry already names the template
as its parent and lists the file under `include`, which composes it on that
template:

```yaml
# layers/duration.yaml
yamaa: "1.0"
type: spec

columns:
  - name: ADURN
    ...
```

Do not move the template into the reusable file instead: the duration would
then serve only that one template.
