# Reject a Package Codelist That Reuses the Study's Name Under Another Code

**Goal:** carry the reason for discontinuation from the study (`DCSREAS`)
into ADSL, restricted to this submission package's own list of reasons.

**Input:** one pre-analysis row per subject carrying the reason the subject
left the study, and two sets of coded values: the study's, which every
submission package of the study shares, and this package's own.

**Variables:**

- `DCSREAS` would be the reason for discontinuation from the study, one of
  the package's reasons.

**Note:** the package means to replace the study's list of reasons, but
declares its own under another code. The study's list is then kept beside
it, and the two lists carry one name, so a reader of the data-definition
document could not tell them apart; the run is rejected before any data is
read.

## How to fix

Decide whether the package changes the study's list or adds a second one.
To change it, declare the package's list under the study's `id`, which
replaces the study's list whole, and bind that `id`:

```yaml
codelists:
  - id: DCSREAS
    name: Reason for Discontinuation
    items:
      - {value: ADVERSE EVENT, decode: ADVERSE EVENT}
```

A list the package adds beside the study's takes a name of its own.
