# T7 Design: private derived datasets

This design names an intermediate row grain only when one atomic artifact
build needs it. A durable or reused intermediate remains the artifact of a
separate specification and is supplied downstream as an ordinary stored
source. File paths do not imply cross-specification execution order.

A `derived` entry builds a private dataset before the artifact. It has its own
driver, row-construction form, keys, and dependency-ordered columns. Every
declared column is visible to its readers, but the dataset is never serialized.
Its key must be non-missing and unique before any dependent dataset can read it.

This closes two concrete gaps:

- `sdtm-ae-effective-transaction` first resolves one effective transaction per
  event, then drives artifact rows only from transactions not marked `REMOVE`.
- `adam-adtr-sum-of-target-diameters` first constructs a private `ASSESS`
  grain, then derives the current nadir from completed prior assessments.

The nadir also extends declared range matching to qualified aggregates. An
aggregate `between` narrows its right-side records with inclusive lower and/or
upper bounds. A missing current-row value admits no records and therefore
returns the empty-group result; it never removes the cutoff.

`negative-adtr-duplicate-assessment` fixes the intermediate identity failure:
a repeated derived key fails before the artifact may consume that dataset.

Private derived datasets are not convenience projections. If direct
expressions can produce a one-to-one result, or if another artifact needs the
intermediate, the specification should not hide it under `derived`.
