# ADaM ADAE: reject an order that places one value twice

This example uses collected adverse events to attempt one record per event:

- `AETERM` is the reported term for the event, and `ASTDT` the date it began.

The records are to be presented by subject, then by onset date with the most
recent first, and then by subject again in the opposite direction. The last
term contradicts the first: the subject cannot both open the order and reverse
it, and whichever the run applied would decide the artifact by accident. One
value takes one place in an order, so the run must fail before any data is read
and no artifact is accepted.

## How to fix

Decide which position the subject holds and state it once. Grouping a subject's
events together while reading the newest first is the first term ascending and
the date descending:

```yaml
output:
  columns: [STUDYID, USUBJID, ASEQ, AETERM, ASTDT]
  order_by:
    - USUBJID
    - variable: ASTDT
      direction: desc
```

Presenting the subjects in reverse instead means declaring `USUBJID`
descending once, in the first position, rather than adding a second term for
it.
