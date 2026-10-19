# Reject a Sex Format Name Without the Dollar Sign

**Goal:** carry the collected sex (`SEX`) for each subject, coded with
the study's controlled terminology for sex.

**Input:** collected demographics with the study and subject
identifiers and the recorded sex code.

**Variables:**

- `SEX` would be the subject's recorded sex, one of the codes the
  terminology lists: `F`, `M` or `U`.

**Note:** the sex terminology also names the SAS format a reader of the
submission applies to its codes. The codes are text, and the name of a
format for text begins with a dollar sign, so a name without one is
rejected before any record is read.

## How to fix

Name the character format the sex codes are read with: begin the
codelist's `format_name` with `$`.

```yaml
- id: SEX
  name: Sex
  alias: C66731
  format_name: $SEX
  items:
    - {value: F, decode: Female, alias: C16576}
    - {value: M, decode: Male, alias: C20197}
    - {value: U, decode: Unknown, alias: C17998}
```

A study that gives its readers no format name for this list omits
`format_name` instead.
