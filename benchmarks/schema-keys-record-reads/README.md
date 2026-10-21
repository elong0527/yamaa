# Each Dose Level a Subject Received, Against the Planned Dose

**Goal:** show that a record's identity may be computed from that one
record, through the project's own routine and through the subject's planned
dose: one record per subject and dose level received (`DOSE`), stating
whether the level is above the subject's planned dose (`ABOVEFL`) and what
that planned dose is (`PLANDOSE`).

**Input:** one `spec.yaml` over exposure records (`input/ex.csv`: one per
administration, with the study day and the dose given) and each subject's
planned dose (`input/plan.csv`). The `project.yaml` lists the function
file `functions/project_value.yaml`, whose routine returns the dose it is
given.

**Variables:**

- `DOSE` is a dose level the subject received, as the project's routine
  returns it.
- `ABOVEFL` is `Y` when that dose is above the subject's planned dose and
  `N` otherwise.
- `PLANDOSE` is the subject's planned dose.

**Note:** each administration contributes its own dose and flag, so a level
the subject received several times gives one record, while a different level
gives another, each in the order of its first administration.
