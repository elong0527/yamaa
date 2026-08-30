# ADaM ADOE: tell the study eye from the fellow eye

This example uses collected ophthalmic measurements and subject-level eye
assignments with a `yamaa` specification to derive one row per collected
measurement:

- `OESEQ` is the collected sequence number that identifies the measurement
  within its subject;
- `PARAMCD` is the measurement, `OELAT` the eye it was taken in, and `AVAL`
  the measured value. A measurement collected without a value is kept and its
  value left empty;
- `AFEYE` is the eye's role in the study: the measurement belongs to the
  study eye when its laterality matches the laterality the study assigned to
  the subject, to both eyes when the measurement is bilateral, and to the
  fellow eye when it is the opposite unilateral eye. Either unilateral eye is
  a study eye for a bilateral assignment. A subject with no assigned eye has
  no role for either eye.

The assignment is a property of the subject, not of the measurement, so the
same eye is the study eye at every visit; only the collected laterality moves
a record between roles.
