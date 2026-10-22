# Reject a Converted Laboratory Result Described as Delivered

**Goal:** build the laboratory domain with one record per subject for
glucose and one for creatinine, each with its result in standard units
(`LBSTRESN`) and the metadata a data definition describes it by.

**Input:** laboratory results delivered by the vendor, one record per
subject, with glucose in mmol/L (`GLUC`) and creatinine in mg/dL
(`CREATMG`).

**Variables:**

- `DOMAIN` is `LB` on every record.
- `LBTESTCD` would be `GLUC` or `CREAT`.
- `LBSTRESN` would be the glucose as delivered, and the creatinine converted
  to micromoles per litre (multiplied by 88.42).

**Note:** the result of each test is described as delivered by the vendor.
The creatinine result is converted here before it is submitted, so it did
not arrive as it stands, and its description contradicts how the value is
made; the run is rejected before any data is read and no artifact is
accepted.

## How to fix

Describe the converted creatinine result as derived by the sponsor, and keep
the delivered glucose result as collected:

```yaml
- id: CREAT
  where: {LBTESTCD: [CREAT]}
  origins:
    - type: Derived
      source: Sponsor
  method: Creatinine in mg/dL multiplied by 88.42
```
