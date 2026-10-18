# Accented Investigator Name in Transport

**Goal:** submit DM with the investigator's name in a SAS transport file.

**Input:** one row per subject carrying the site, age, sex and investigator
name, one of which is spelled with accented letters.

**Variables:**

- `SITEID`, `AGE` and `SEX` are as collected.
- `INVNAM` is the investigator's name as collected.

**Note:** a submitted text value is printable ASCII, so the value with an
accented letter stops the submission instead of being altered.

## How to fix

Correct the name in the collected data to its ASCII spelling, as the
investigator agrees it should be submitted, and keep the accented spelling
outside the submission. Alternatively map the collected spelling explicitly:

```yaml
    derivation:
      mapping:
        source: DM_RAW.INVNAM
        dict: investigator_names.yaml
```
