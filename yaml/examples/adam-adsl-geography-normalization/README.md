# ADaM ADSL: normalize collected country and group it into a region

This example uses sample DM data and a `yamaa` specification to derive one row
per subject:

- `COUNTRY` is the collected country code in upper case, so the same country
  reported in different cases becomes one value. A subject with no collected
  country is `UNKNOWN`;
- `REGION1` is the region the country belongs to. Countries the study does not
  map to a named region, `UNKNOWN` among them, fall into `Rest of World`, so
  every subject has a region.
