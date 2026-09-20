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

**Columns:** every output column traces to one source variable.

| Column  | Label                      | Type  | Source      |
|---------|----------------------------|-------|-------------|
| USUBJID | Unique Subject Identifier  | str   | LB.USUBJID  |
| PARAMCD | Parameter Code             | str   | LB.LBTESTCD |
| AVAL    | Analysis Value             | float | LB.LBSTRESN |
| AVALU   | Standardized Analysis Unit | str   | LB.LBSTRESU |

**Assumptions:**

1. Inheritance Definition: three levels share one mapping. The
   organization level declares the common column mapping, the
   compound level refines it, and the study level is the entry
   that declares the final output.
2. A parent named by two levels resolves once. The compound level
   names the organization spec and the study level names both;
   resolution runs depth-first, left to right, and the deepest
   level that sets a wording wins.
3. The AVAL label is retitled at every level: "Standardized
   Numeric Result" at the organization level, "Compound
   Standardized Result" at the compound level, "Analysis Value"
   at the study level. The study wording wins and is the label
   in the dataset.
4. Only columns named in the study output reach the dataset. The
   organization-level literal, the unused input, and the unused
   intermediate are absent from the resolved spec and the
   dataset.
5. One record per subject per parameter: the `USUBJID` and
   `PARAMCD` pair is unique, and a duplicate pair is an error.

**Example:** two laboratory rows, one per subject.

Row 1: subject `01`, parameter `ALT`, analysis value 12.5 U/L.
Row 2: subject `02`, parameter `ALT`, analysis value 21.0 U/L.

**Standard:** ADaM | **Domain:** ADLB
