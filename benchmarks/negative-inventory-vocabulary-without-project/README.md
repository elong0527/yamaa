# Reject Source Domains Checked Without the Study's Dataset List

**Goal:** carry each adverse event's source domain (`SRCDOM`) and source
sequence number (`SRCSEQ`) onto the analysis dataset.

**Input:** reported adverse events with the study and subject identifiers,
the domain they belong to, a sequence number and the reported term.

**Variables:**

- `SRCDOM` would be the name of the dataset each record comes from, which
  must be a dataset the study defines.
- `SRCSEQ` would be the record's sequence number in that dataset.

**Note:** the datasets a study defines are listed once, in the study's own
file, beside the specifications that build them. Built on its own, the
analysis dataset has no list to check its source domains against, so the run
is refused before any record is read instead of accepting source domains that
nobody checked.

## How to fix

Build the dataset as part of its study: open it from the study's project
file, which lists the SDTM dataset the source domains name, for example
`yamaa_project("project.yaml")$spec("ADAE")$run()` in R,
`yamaa.project("project.yaml").spec("ADAE").run()` in Python, or
`yamaa run project.yaml --spec ADAE` on the command line:

```yaml
yamaa: "1.0"
type: project
sdtm:
  specs: [../sdtm/ae.yaml]
adam:
  specs: [spec.yaml]
```

Every `SRCDOM` value must then be the domain of a specification the project
lists, here `AE`.
