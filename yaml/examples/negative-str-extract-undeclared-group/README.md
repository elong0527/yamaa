# ADaM ADSL: reject a site identifier taken from an uncaptured part

This example uses collected demographics to record one row per subject:

- `SITEID` is the study site identifier read out of the subject identifier.

The subject identifier is read by a text rule that sets aside one part for
reuse, but the site is taken from a second part the rule never sets aside.
No implementation has a value to return, so the specification is rejected
before any data is read and no artifact is accepted.

## How to fix

Decide which part of the subject identifier names the site, then take the part
the rule actually sets aside. Parts are numbered from 1 in the order their
opening parentheses appear, so the site here is part 1:

```yaml
str_extract:
  source: USUBJID
  pattern: '^CATH-([^-]+)-[0-9]{4}$'
  group: 1
```

Set aside a second part when the correction genuinely needs one, and number it
by that same position. Here part 2 is the four-digit subject number:

```yaml
str_extract:
  source: USUBJID
  pattern: '^CATH-([^-]+)-([0-9]{4})$'
  group: 2
```
