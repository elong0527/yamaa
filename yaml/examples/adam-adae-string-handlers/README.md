# ADaM ADAE: clean text and handle invalid IDs

This example uses sample AE data and a `yamaa` specification to:

- convert `AETERM` to lowercase as `AETERMLO`;
- convert `AEREL` to lowercase as `AERELLC`, using `not reported` when
  `AEREL` is missing;
- convert `AERELLC` to uppercase as `AREL` and verify its allowed values;
- extract the number from an `AESPID` such as `AE-001` as `AEREFNUM`. A
  missing `AESPID` becomes `0`, and an invalid `AESPID` becomes `-1`.

An `AESPID` recorded as blank is a collected answer rather than an absent
one, so it is an invalid identifier and its `AEREFNUM` is `-1`, not the `0`
that stands for an identifier nobody recorded. The two blanks stay apart
in the result: a recorded blank is written as an empty pair of quotation
marks and an uncollected one as nothing at all.
