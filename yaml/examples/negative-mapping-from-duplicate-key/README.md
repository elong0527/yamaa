# ADaM ADLB: reject a reference range stated twice

This example uses collected laboratory results with a table of reference limits
by test and sex to attempt one record per subject and parameter:

- `SEX` is the sex the limits are chosen by;
- `AVAL` is the collected result;
- `ANRHI` is the upper limit of normal for that test and sex.

The limit table gives two different upper limits for the same test and sex.
Taking either one, or the first the file happens to list, would make the result
depend on the order of a file rather than on the study's reference ranges, so
the run must fail and no artifact is accepted.

## How to fix

Make the lookup table unique on `[LBTESTCD, SEX]` by resolving the conflicting
`ALT/F` reference limits under the study's governed reference-range rules. A
`mapping_from` lookup cannot choose one duplicate by file order.

If both rows are valid for different conditions, add the distinguishing field
to both the current-row `source` list and the lookup `key` list. For example, a
method-specific table would use matching lists such as
`source: [PARAMCD, SEX, METHOD]` and `key: [LBTESTCD, SEX, METHOD]`.
