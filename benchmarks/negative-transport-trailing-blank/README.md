# Site Identifier with a Trailing Blank in Transport

**Goal:** submit DM in a SAS transport file.

**Input:** one row per subject carrying the site, age and sex; one site
identifier was entered with a trailing blank.

**Variables:**

- `SITEID` is the collected site identifier, blank included.
- `AGE` and `SEX` are as collected.

**Note:** a SAS transport file pads text with blanks, so a value ending in a
blank cannot be read back as written and stops the submission.

## How to fix

Correct the site identifier in the collected data. When the source cannot be
corrected, remove trailing blanks explicitly:

```yaml
  - name: SITEID
    type: str
    label: Study Site Identifier
    derivation:
      str_replace:
        source: DM_RAW.SITEID
        pattern: " +$"
        replacement: ""
```
