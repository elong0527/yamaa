# Inherited Output

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-inherited-output.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** show a study inheriting the whole artifact layout from the
level it shares with other studies, so the list of published
variables is written once rather than copied into every study.

**Dataset:** `adsl.csv`, ADSL subject-level dataset. One record per
subject.

**Input:** two spec files, read root-first:

- `spec_shared.yaml` carries what every study publishing this table
  must agree on: the dataset, its keys, the four columns, and the
  complete `output` naming them in order;
- `spec_study.yaml` is the entry: it names the shared level as its
  parent and supplies only the study's own demographics file. It
  declares no `output`.

The chain resolves to `expected/spec_resolved.yaml`, the default view
in the specification dropdown.

**Variables:**

- `AGE` and `AGEU` carry each subject's collected age and its units
  from demographics, as the shared level declares them.

**Note:** `output` is inherited whole. A study that writes its own
`output` replaces all of it, variable list included, and an inherited
`output.path` names a file beside the level that wrote it. Here both
levels share one directory, so the study publishes `adsl.csv` beside
itself.

**Standard:** ADaM | **Domain:** ADSL
