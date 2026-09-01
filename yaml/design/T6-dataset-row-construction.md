# T6 Design: counted row expansion

This design closes the data-dependent row-count gap with one focused
alternative to ordinary row templates: counted expansion. Grouped construction
remains a mode of an individual row template through `rows[].group_by`; this
design does not introduce a second root-level spelling for it.

`expand` constructs a data-declared number of rows per base record:

```yaml
base: EX
expand:
  count: EX.EXDOSCNT
  index: ADOSEN
```

`count` must name a qualified base field and resolve to a non-missing,
non-negative integer. `index` names the declared integer column that receives
the values from one through that count. A zero count contributes no row, while
a missing count fails rather than silently erasing a record.

`adam-adex-single-dose-expansion` replaces a rejected fixed set of row
templates and proves that a record containing five administrations produces
five rows without an authored maximum. `negative-adex-missing-dose-count`
fixes the missing-count failure contract. `adam-advs-once-measured-carry-forward`
expands the planned subject/parameter spine before matching collected values,
so an unattended visit still has an analysis row.
