# Sex Label Too Long for Transport

**Goal:** submit DM in a SAS transport file.

**Input:** one row per subject carrying the site, age and sex.

**Variables:**

- `SITEID`, `AGE` and `SEX` are as collected.

**Note:** a submitted variable label has at most 40 characters, so the
submission stops before any file is written; no label is shortened.

## How to fix

Use the label the implementation guide publishes, `Sex`, and keep any longer
explanation in a comment, which the data-definition document carries in full:

```yaml
  - name: SEX
    type: str
    label: Sex
```
