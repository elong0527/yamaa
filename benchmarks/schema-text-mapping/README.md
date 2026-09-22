# Text Mapping

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-text-mapping.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** pin the text lookup contract: inline dictionaries, an external
dictionary file, case-insensitive reads, and a regular-expression
search with capture groups.

**Input:** `DM` carries four subjects with coded sex, race, and
treatment group values, plus site-prefixed subject identifiers. One
subject carries codes outside every dictionary, and one has missing
values throughout.

**Columns:**

- `SEXC` reads an inline dictionary with exact matching. The coded
  values translate; the unmapped code and the missing value both read
  as the declared missing value.
- `RACEC` uses the same lookup contract with the dictionary loaded
  from a YAML file beside the input instead of written in the spec.
- `TRTGRPC` reads case-insensitively: source and dictionary keys are
  folded before comparison, so lowercase and mixed-case treatment
  codes match their uppercase keys.
- `SITENUM` searches each identifier with a regular expression and
  keeps the second capture group. Identifiers with the site prefix
  yield their numeric part; the identifier with no match yields the
  declared no-match value, and the missing identifier yields the
  declared missing value.

**Standard:** CDISC | **Domain:** ADSL
