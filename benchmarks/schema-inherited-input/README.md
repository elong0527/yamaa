# Inherited Input Types

**Goal:** demonstrate a study adding to the demographics input it inherits
from a shared level, keeping the shared file and field type, to derive
`AGEGR1` and `HTGR1`.

**Input:** two spec files, read root-first:

- `spec_shared.yaml` names the demographics file once, stores `AGE` as an
  integer, and groups subjects by age;
- `spec_study.yaml` is the entry: it names the shared level as its parent,
  stores the height it collects as a number, groups subjects by height, and
  declares the complete output.

**Variables:**

- `AGEGR1` is `<65` or `>=65` from the collected age; a subject aged exactly
  65 falls in the older group;
- `HTGR1` is `<170` or `>=170` from the collected height in centimetres; a
  height of exactly 170 falls in the taller group.

**Note:** the study's entry for the demographics input joins the shared one
detail by detail: the file location and the age type come from the shared
level, and the height type from the study. Neither level restates what the
other declares, and both groupings read numbers.
