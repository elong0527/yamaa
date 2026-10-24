# Reject a quantile beyond the greatest value

**Goal:** carry each laboratory result into `AVAL` and add `AVALFNC`, the
upper outlier fence of the parameter's results across subjects.

**Input:** one alanine aminotransferase (ALT) result per subject.

**Variables:**

- `AVALFNC` would be the parameter's upper outlier fence, the third quartile
  plus 1.5 times the interquartile range. The request instead asks for the
  quantile at 1.5, and a quantile is a proportion of the sorted values from
  0 through 1, so the run is rejected before any data is read.

## How to fix

State the fence as the formula it is: the third quartile plus 1.5 times the
distance between the first and third quartiles.

```yaml
- name: AVALFNC
  type: float
  derivation:
    aggregate:
      group_by: [PARAMCD]
      expr: "Q3(AVAL) + 1.5 * (Q3(AVAL) - Q1(AVAL))"
```

If a high percentile was meant instead, write it as a proportion, such as
`quantile: 0.95` beside `QUANTILE(AVAL)` for the 95th percentile.
