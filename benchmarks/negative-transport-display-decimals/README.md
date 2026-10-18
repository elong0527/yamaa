# Display Digits Written into Submitted Heights

**Goal:** carry `HEIGHTBL` into ADSL and submit it as the SAS transport file
`adsl.xpt`.

**Input:** one pre-analysis row per subject carrying the baseline height in
centimetres, measured to two decimals.

**Variables:**

- `HEIGHTBL` is the baseline height in centimetres, written with one decimal;
  missing when not measured.

**Note:** the transport file carries the heights the published data holds, so
heights written with one decimal for display would be submitted rounded. The
submission stops before any file is written.

## How to fix

Keep the measured heights in the data, so 172.46 travels as 172.46 rather
than 172.5, and leave the one-decimal display to the display format the
data-definition document already reports, which changes no value. Remove the
artifact's display digits:

```yaml
output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, HEIGHTBL]
```

If the analysis itself uses the height to one decimal, round it as part of
the value and describe that rounding in the column's method:

```yaml
    derivation:
      round_half_away_from_zero: {source: ADSL_RAW.HEIGHTBL, digits: 1}
```
