# Empty Investigator Name in Transport

**Goal:** submit DM with the investigator's name in a SAS transport file.

**Input:** a Parquet export with one row per subject carrying the site, age,
sex and investigator name; the export stores an unanswered name as an empty
string, and the input reads it as a present value.

**Variables:**

- `SITEID`, `AGE` and `SEX` are as collected.
- `INVNAM` is the investigator's name; empty for the unanswered one.

**Note:** a SAS transport file cannot tell an empty string from a missing
value, so the present empty value stops the submission instead of turning
into a missing one.

## How to fix

Decide that an unanswered name is missing, which is how SDTM submits it, and
read the stored empty string that way. `missing` is the default, so remove
the declaration:

```yaml
input:
  DM_RAW: input/dm_raw.parquet
```
