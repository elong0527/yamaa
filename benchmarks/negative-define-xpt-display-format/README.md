# Refuse a SAS Transport File With an Unreadable Display Format

**Goal:** submit the demographics domain as a SAS transport file with its
data definition, the age (`AGE`) displayed with three digits and no
decimals.

**Input:** collected demographics with the age in years, one record per
subject.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `AGE` is the age in years at informed consent.

**Note:** the age's display format is written `3,0`, with a comma between
width and decimals. A SAS format separates them with a period, so the
transport file could not carry the format, and the submission is refused
before any file is written; the dataset itself still builds.

## How to fix

Write the format as SAS spells it, width then a period:

```yaml
display_format: "3."
```

`3.0` states the same display with its zero decimals written out.
