# Inherited Output

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

**Note:** the study writes no `output`, so it inherits all of it. A
study that writes one part of `output`, such as its own variable list,
keeps every part it leaves out, and an inherited `output.path` names a
file beside the level that wrote it. Here both levels share one
directory, so the study publishes `adsl.csv` beside itself.
