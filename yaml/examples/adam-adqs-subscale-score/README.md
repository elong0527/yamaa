# ADaM ADQS: score a questionnaire subscale from its item records

This example uses collected questionnaire responses and a `yamaa`
specification to derive one row per subject, visit, and analysis parameter:

- `PARAMCD` and `PARAM` name the parameter. Every item of the physical
  functioning scale becomes one, and the subscale score those items support
  becomes one more;
- `AVAL` is the collected response on an item record, on the instrument's
  zero-to-four answer scale, and the subscale score on the score record. The
  score is the mean of the items answered at that visit, placed on a
  zero-to-one-hundred scale, and is empty when fewer than three of the four
  items were answered.

A visit at which the questionnaire was administered carries a score record
even when too few items were answered to score it, and a visit at which it was
not administered carries no records at all. An unscorable visit is therefore
still distinguishable from one the subject never reached.
