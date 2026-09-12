# SDTM DM: declare the metadata a submission needs

This example uses collected DM data to produce one record per subject. `SITEID`
is the collected site identifier:

- `USUBJID` combines the study, site, and subject identifiers;
- `AGE` is the collected age and must fall between 0 and 120;
- `AGEU` is `YEARS`;
- `SEX` is the collected sex and must be `F`, `M`, or `U`;
- `COUNTRY` is the collected three-letter country code.

The dataset and its variables carry the description, labels, provenance,
lengths, terminology, class, structure, and standard version a submission
review needs, and the example carries the data-definition document those
declarations produce beside the data itself. A subject identifier longer than
30 characters is rejected rather than shortened.
