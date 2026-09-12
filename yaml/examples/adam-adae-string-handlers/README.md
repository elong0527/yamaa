# ADaM ADAE: clean text and handle invalid IDs

This example uses sample AE data and a `yamaa` specification to:

- convert `AETERM` to lowercase as `AETERMLO`;
- convert `AEREL` to lowercase as `AERELLC`, using `not reported` when
  `AEREL` is missing;
- convert `AERELLC` to uppercase as `AREL` and verify its allowed values;
- extract the number from an `AESPID` such as `AE-001` as `AEREFNUM`. A
  missing `AESPID` becomes `0`, and an invalid `AESPID` becomes `-1`.

A blank `AESPID` is missing whether it was recorded bare or quoted, so it
becomes the `0` that stands for an identifier nobody recorded.
