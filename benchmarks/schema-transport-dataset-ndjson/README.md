# Newline-Delimited Dataset-JSON

**Goal:** write the demographics dataset of `sdtm-dm-metadata` as a
Dataset-JSON 1.1 file in the standard's newline-delimited representation,
`dm.ndjson`, beside the data-definition document that points at it.

**Input:** the `sdtm-dm-metadata` specification, project and input, with the
project's `sdtm` section setting `transport: dataset_ndjson` in place of
`transport: dataset_json`.

**Variables:** as `sdtm-dm-metadata` declares them; the dataset's rows, its
column metadata and its entry in the data-definition document are unchanged.

- `USUBJID` joins the study, site, and subject identifiers with hyphens;
  an identifier longer than 30 characters stops the run rather than being
  shortened.
- `SITEID` is the collected site identifier.
- `AGE` is the collected age in whole years, between 0 and 120;
  missing when not collected.
- `AGEU` is fixed to `YEARS`, since age is collected in whole
  years.
- `SEX` is the collected sex, one of `F`, `M`, or `U`.
- `COUNTRY` is the collected three-letter country code.

**Note:** the first line of `dm.ndjson` is the metadata object of
`sdtm-dm-metadata`'s `dm.json` without its `rows`, written on one line, and
each later line is one row, written exactly as `dm.json` writes it. A
receiver reads the file one JSON text at a time; the content is the same
dataset, and the document's leaf names `dm.ndjson`.
