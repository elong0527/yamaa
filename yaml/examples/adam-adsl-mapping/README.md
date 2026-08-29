# ADaM ADSL: translate collected values into a standard vocabulary

This example uses sample DM data and a `yamaa` specification to derive one row
per subject:

- `SEX` is the collected sex in standard form, `M` or `F`, read without regard
  to the case it was reported in. A sex that was not collected, and one the
  study does not recognise, both become `U`;
- `SEXN` and `SEXDECOD` are the same value as a number and as display text,
  `1` and `Male` for men, `2` and `Female` for women, with `0` and `Unknown`
  for a sex that resolved to `U`;
- `RACE` is the collected race unchanged, and `RACEN` its numeric code. Race is
  matched exactly as reported, so a race the study does not code, such as a
  multiple-race response, becomes `99`;
- `AGE` is the collected age as a whole number, empty when it was not collected
  and also empty when what was collected is not a number;
- `AGEGR1` is the age band the subject falls in: under 18, 18 to 64, or 65 and
  over, with `UNKNOWN` when there is no age.

One collected value can feed several output variables at once, and each carries
its own translation: sex produces three, in three different vocabularies.
