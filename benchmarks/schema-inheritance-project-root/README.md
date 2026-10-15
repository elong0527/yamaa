# Shared Input, Project Data

**Goal:** demonstrate a shared level declaring a study's demographics
input once - its name and the `AGE` field type - while the stored file
stays with the study's project, and derive `AGE` from it.

**Input:** two spec files and the project's demographics:

- `common/spec_common.yaml` is the shared level, kept in its own
  directory as it would be when several studies inherit it. It names
  the demographics file `../input/dm.csv`, gives the `AGE` field type,
  and declares every column;
- `spec_study.yaml` is the entry: it names the shared level as its
  parent and declares the complete output;
- `input/dm.csv` sits at the project root (this benchmark's
  directory), not beside the shared level.

**Note:** paths resolve from the directory of the file that declares them.
The shared level writes `../input/dm.csv`, which reaches the study's input
from `common/`. The engine resolves that declaring-file path once.

`expected/spec_resolved.yaml` retains the historical reference view. It is
excluded from public conformance until cutover under #1973 item 14.
