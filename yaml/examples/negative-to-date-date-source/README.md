# ADaM ADAE: reject extracting a date from a date

The collected adverse event attempts to produce one analysis row:

- `ASTDT` is the collected calendar date;
- `ASTDT2` is rejected because extracting a calendar date requires a local
  datetime, while its source is already a date.

## How to fix

Use the date directly. When the source is a datetime, extract its calendar date
explicitly:

```yaml
- name: ASTDT2
  type: date
  derivation:
    to_date: {source: ASTDTM}
```
