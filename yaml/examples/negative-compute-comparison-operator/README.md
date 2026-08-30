# ADaM ADLB: reject a comparison inside `compute`

This example uses collected laboratory results with their reference limits to
attempt one record per subject and parameter:

- `AVAL` is the collected result and `ANRHI` the upper limit of its reference
  range;
- `HIFL` is meant to mark a result above that limit.

The intended result is reasonable: for these input rows, `HIFL` would be `1`
when `AVAL` is greater than `ANRHI` and `0` otherwise. The error is how that
result is expressed. `compute` accepts only numeric arithmetic, so the
comparison operator in `AVAL > ANRHI` is prohibited. Declaring `HIFL` as
`type: int` does not implicitly convert the comparison's Boolean answer to
`1` or `0`.

The specification must make that conversion explicit with `case`, whose
`when` field accepts a comparison and whose branches return numeric literals:

```yaml
- name: HIFL
  type: int
  derivation:
    case:
      branches:
        - when: "AVAL > ANRHI"
          then:
            literal: 1
      otherwise:
        literal: 0
```

This remains a negative example because it deliberately uses the invalid
`compute` form and therefore must fail during validation without producing an
artifact.
