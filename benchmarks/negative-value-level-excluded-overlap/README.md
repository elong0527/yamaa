# Reject Two Descriptions That Each Exclude the Other's Tests

**Goal:** submit the laboratory results (`LBTESTCD`, `LBTEST`, `LBORRES`),
describing the results the site tests apart from all the others.

**Input:** laboratory results, one row per test per subject.

**Variables:**

- `LBTESTCD` and `LBTEST` are the test's code and name.
- `LBORRES` is the result as reported, in the reporting laboratory's units.

**Note:** the central laboratory's results are described as the tests other
than the site's two, and the site's results as the tests other than the
central laboratory's two. A test that is none of the four, or a result
without a test code, would then fit both descriptions, so they are rejected
before any data is read.

## How to fix

Name the site's tests in its description, so that the central laboratory's
description, which excludes them, takes every other test:

```yaml
- id: SITE
  where: {LBTESTCD: [HCG, PH]}
```

Exclude a list of values in one description only; two exclusions on one
column always share the rows that hold neither list, a missing value among
them.
