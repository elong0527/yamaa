# Spec Inheritance

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/spec-inheritance.html) [![Lifecycle: finalized](https://img.shields.io/badge/Lifecycle-finalized-brightgreen)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** show how a study spec inherits from its organization and
compound parents: shared levels carry the common mapping once, and
the deepest study level wins on final wording.

**Dataset:** `adlb.csv`, ADLB analysis dataset. One record per
subject per laboratory parameter.

**Input:** three spec files, read root-first:

- `spec_organization.yaml` maps the laboratory test code, result,
  and unit into `PARAMCD`, `AVAL`, and `AVALU`;
- `spec_compound.yaml` adds field types and retitles the AVAL
  result;
- `spec_study.yaml` is the entry: it names both levels as parents,
  restates the AVALU unit wording, and declares the complete
  output.

Parents resolve depth-first, left to right, with later levels
winning; the chain resolves to `expected/spec_resolved.yaml`, the
default view in the specification dropdown.

