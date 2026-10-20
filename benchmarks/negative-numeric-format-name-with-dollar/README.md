# Reject an Age Group Format Name With the Dollar Sign

**Goal:** carry the pooled age group code (`AGEGR1N`) for each subject,
coded with the study's terminology for age groups.

**Input:** one pre-analysis row per subject with the study and subject
identifiers and the pooled age group code.

**Variables:**

- `AGEGR1N` would be the subject's pooled age group, one of the codes the
  terminology lists: `1` below 65 years, `2` from 65 through 80 years and
  `3` above 80 years.

**Note:** the age group terminology also names the SAS format a reader of
the submission applies to its codes. The codes are numbers, and a format
whose name begins with a dollar sign is a format for text, which cannot be
applied to a number, so the name is rejected before any record is read.

## How to fix

Name the numeric format the age group codes are read with: drop the leading
`$` from the codelist's `format_name`.

```yaml
- id: AGEGR1N
  name: Pooled Age Group 1 (N)
  data_type: integer
  format_name: AGEGRN
  items:
    - {value: 1, decode: '<65'}
    - {value: 2, decode: '65-80'}
    - {value: 3, decode: '>80'}
```

A study that gives its readers no format name for this list omits
`format_name` instead.
