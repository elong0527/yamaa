# SDTM FA: fever occurrence

Reactogenicity temperature records from VS produce one FA fever-occurrence
record at the source-record grain:

- `FASEQ` preserves the collected VS sequence number and its row identity;
- `FATESTCD`, `FATEST`, `FACAT`, `FASCAT`, and `FAOBJ` identify a
  systemic reactogenicity fever occurrence;
- `FAORRES` is `Y` when the Celsius result is 38 or higher, `N` when it is
  lower, and missing when the numeric result is missing or its unit is not
  Celsius;
- `FASTRESC` matches `FAORRES`;
- `VSSTRESN` preserves the numeric result used for the threshold.

This covers the upstream branch in which no fever FA record already exists.
