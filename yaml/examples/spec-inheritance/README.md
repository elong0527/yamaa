# Spec Inheritance

**Goal:** demonstrate spec inheritance from organization to compound to
study level.

**Input:** three spec files, read root-first:

- `spec_organization.yaml` maps the laboratory test code, result, and
  unit into `PARAMCD`, `AVAL`, and `AVALU`;
- `spec_compound.yaml` adds field types and retitles the AVAL result;
- `spec_study.yaml` is the entry: it names both levels as parents,
  restates the AVALU unit wording, and declares the complete output.

Parents resolve depth-first, left to right, with later levels winning;
the chain resolves to `expected/spec_resolved.yaml`, the default view
in the specification dropdown.

**Note:** shared levels carry the common renaming once, and the deepest
study level wins on final wording.

**Standard:** ADaM | **Domain:** ADLB

[Rendered view](https://elong0527.github.io/yamaa/examples/spec-inheritance.html)
