# Spec Column Composition

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/spec-column-composition.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** demonstrate a study level changing one detail inside an
inherited column - `AVAL`, `ANRIND`, and `PARCAT1` - without restating
the rest of that column.

**Input:** three spec files, read root-first:

- `spec_organization.yaml` turns the laboratory test code, result, and
  reference range indicator into `PARAMCD`, `AVAL`, `ANRIND`, and
  `PARCAT1`, reading the indicator without regard to letter case;
- `spec_compound.yaml` adds the low reference range wording the
  organization does not list, and records the laboratory `AVAL` is
  collected by;
- `spec_study.yaml` is the entry: it applies one corrected result,
  leaves an unrecognized indicator empty, states `PARCAT1` as the one
  category this study collects, and declares the complete output.

Parents resolve depth-first, left to right, with later levels winning;
the chain resolves to `expected/spec_resolved.yaml`, the default view
in the specification dropdown.

**Note:** a later level composes a column detail by detail. The added
reference range wording joins the two the organization already lists,
the three annotations on `AVAL` accumulate, and the case-insensitive
reading survives a level that never mentions it. A level naming a
different way to produce a value replaces it whole instead, which is
why `PARCAT1` stops reading the test code. Lists never accumulate: the
study's own check on `AVAL` replaces the organization's.

**Standard:** ADaM | **Domain:** ADLB
