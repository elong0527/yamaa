# Reject Sex Codes Checked Without the Study's Terminology

**Goal:** carry each subject's sex (`SEX`), coded with the study's sex
terminology.

**Input:** collected demographics with the study and subject identifiers and
the sex as collected.

**Variables:**

- `SEX` would be the subject's sex as collected, one of the codes the
  study's sex terminology lists.

**Note:** the study declares its sex terminology once, beside its list of
datasets, and every dataset that codes sex refers to it. Built on its own,
the demographics dataset has no terminology to check its codes against, so
the run is refused before any record is read instead of publishing codes
that nobody checked.

## How to fix

Build the dataset as part of its study: open it from the study's project
file, which declares the sex terminology and lists the dataset, for example
`yamaa_project("project.yaml")$spec("DM")$run()` in R,
`yamaa.project("project.yaml").spec("DM").run()` in Python, or
`yamaa run project.yaml --spec DM` on the command line:

```yaml
yamaa: "1.0"
type: project
codelists:
  - codelists:
      - id: SEX
        name: Sex
        data_type: text
        items: [{value: F}, {value: M}, {value: U}]
sdtm:
  specs: [spec.yaml]
```

A study without a project file yet needs one before its datasets can refer to
shared terminology.
