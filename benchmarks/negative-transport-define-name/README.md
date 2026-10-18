# Transport Beside a Document Not Named define.xml

**Goal:** submit DM in a SAS transport file beside its data-definition
document.

**Input:** one row per subject carrying the site, age and sex.

**Variables:**

- `SITEID`, `AGE` and `SEX` are as collected.

**Note:** transport files sit beside a data-definition document named
`define.xml`, so a document given another file name stops the submission
before any file is written.

## How to fix

Name the document `define.xml`, and give each submission its own directory
when several share a folder:

```yaml
sdtm:
  define: define.xml
  datasets: xpt
```
