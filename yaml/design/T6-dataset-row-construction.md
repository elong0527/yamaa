# T6 Design: dataset-level row construction

This design closes the fixed-row-count gap by giving an artifact two focused
alternatives to ordinary row templates: grouped rows and counted expansion.
The three forms are mutually exclusive.

`group_by` constructs one row for each distinct tuple of declared base fields.
The tuple values are the base fields available directly on that row, while an
aggregate over the base reduces the records in the current group. This makes a
change of grain explicit without inventing a private wrapper dataset when the
grouped grain is already the artifact.

`expand` constructs a data-declared number of rows per base record:

```yaml
base: EX
expand:
  count: EX.EXDOSCNT
  as: ADOSEN
```

`count` must resolve to a non-missing, non-negative integer. `as` names the
declared integer column that receives the values from one through that count.
A zero count contributes no row, while a missing count fails rather than
silently erasing a record.

`adam-adex-single-dose-expansion` replaces a rejected fixed set of row
templates and proves that a record containing five administrations produces
five rows without an authored maximum. `negative-adex-missing-dose-count`
fixes the missing-count failure. `adam-advs-once-measured-carry-forward`
expands the planned subject/parameter spine before matching collected values,
so an unattended visit still has an analysis row.
