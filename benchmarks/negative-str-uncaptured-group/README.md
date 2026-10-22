# Reject Uncaptured Group

**Goal:** read `SITEID` out of the subject identifier for each
subject record.

**Input:** collected demographics with one record for each subject,
carrying the subject identifier.

**Variables:**

- `SITEID` would hold the site code, the text between `CATH-` and the
  four-digit subject number in an identifier matching
  `^CATH-([^-]+)-[0-9]{4}$`, and would be blank when the subject
  identifier is blank or does not fit that form.

The request asks for a second part of that match, but the match sets
aside only one, the site code. The request is rejected before any data
is read, and no artifact is accepted.

## How to fix

Decide which part of the subject identifier names the site, then take the
part the rule actually sets aside. Parts are numbered from 1 in the order
their opening parentheses appear, so the site here is part 1:

```yaml
str_extract:
  source: USUBJID
  pattern: '^CATH-([^-]+)-[0-9]{4}$'
  group: 1
  no_match: null
```

Setting aside the subject number as well does not move the site: its
opening parenthesis still comes first, so the site stays part 1 and the
subject number is part 2:

```yaml
str_extract:
  source: USUBJID
  pattern: '^CATH-([^-]+)-([0-9]{4})$'
  group: 1
  no_match: null
```

Keep the blank results for a blank or ill-formed identifier; dropping them
would stop the run on such an identifier instead.
