# T5 Design: declared range matching

This design closes the interval-join gap without adding a general predicate
over two datasets. A record lookup may narrow its equality-matched records with
one current-row value and one or two bounds from the lookup dataset:

```yaml
record_lookups:
  - id: AWIN
    dataset: AWINDOW
    source: [STUDYID]
    key: [STUDYID]
    between:
      value: ADY
      lower: AWLO
      upper: AWHI
    unmatched: missing
```

The comparisons are fixed and inclusive: `lower <= value` and
`value <= upper`. Either bound may be omitted. A missing current-row value is
an incomplete lookup; a right-side record missing a declared bound is
ineligible; and a complete value with no eligible record is unmatched.

This shape is intentionally narrower than an arbitrary two-sided filter. The
value is a dependency of every column that reads the lookup, while the bounds
must be columns of the lookup dataset. A reviewer can therefore see both the
join relation and its endpoint policy directly.

`sdtm-vs-visit-study-day` assigns an epoch from the study's epoch table, and
`adam-advs-analysis-window-table` assigns analysis visits from the study's
window table. Together they cover closed intervals, one-sided intervals,
unmatched values, and a missing current-row value.

Row-relative aggregate narrowing is a separate extension of this same shape.
It belongs with the private intermediate-grain work because that work provides
the example that needs it.
