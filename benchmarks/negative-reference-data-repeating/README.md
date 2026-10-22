# Reject Reference Data Declared as Repeating

**Goal:** build the trial elements domain, one record per planned element of
the trial, with the metadata a data definition describes it by.

**Input:** the protocol's planned elements, one record per element.

**Variables:**

- `DOMAIN` is `TE` on every record.
- `ETCD` would be the short code of the element.
- `ELEMENT` would be the description of the element.

**Note:** the dataset is declared as reference data, which describes the
trial rather than any subject, and also as repeating, which means more than
one record per subject. Reference data has no subjects to repeat for, so the
two declarations contradict each other, and the run is rejected before any
data is read; no artifact is accepted.

## How to fix

Keep the reference data declaration, and declare the dataset non-repeating:

```yaml
submission:
  label: Trial Elements
  class: TRIAL DESIGN
  structure: One record per planned Element
  repeating: false
  reference_data: true
```
