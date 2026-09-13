# ADaM ADAE: reject an unlisted tie-numbering method

This example uses collected adverse events to record one row per event:

- `AESEV` is the reported severity of the event.
- `SEVRANK` numbers the events by severity, sharing one number across
  ties so equally severe events compare as equal.

The numbering method arrives as a structured value instead of one of
the two named methods. No implementation may guess which method a
mapping means, so the specification is rejected before any data is read
and no artifact is accepted.

## How to fix

Decide how ties consume numbers, then name the method. When equally
severe events share the lowest position they occupy and the next
severity continues after the gap, write it plainly:

```yaml
- name: SEVRANK
  type: int
  derivation:
    rank:
      method: competition
      group_by: [STUDYID, USUBJID]
      order_by:
        - {variable: AESEV, direction: desc}
```

When no numbers may be skipped, name the dense method instead. Do not
encode the choice in a structure the vocabulary does not define.

[Rendered view](https://elong0527.github.io/yamaa/examples/negative-rank-invalid-method.html)
