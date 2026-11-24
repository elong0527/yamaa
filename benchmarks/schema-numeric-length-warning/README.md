# Report Ages Wider Than the Width Reviewers Are Told

**Goal:** carry each subject's age in whole years (`AGE`) into the
demographics domain, and report the ages wider than the width the
specification declares for them.

**Input:** collected demographics with the study and subject identifiers
and the age in whole years, empty when it was not recorded.

**Variables:**

- `STUDYID` and `USUBJID` are the study and subject identifiers, as
  collected.
- `AGE` is the collected age in years, unchanged. It is described to
  reviewers as one digit wide, so an age of ten years or more is wider than
  that description. Every age is checked against it once the variable is
  filled in. A wider age is a warning, not a failure: the run still
  succeeds and publishes every age as collected. An age that was not
  recorded is left empty and is not checked.

The run report (`dm-checks.csv`) has one row per item the run handled, each
naming the dataset (`ARTIFACT`), the kind of item (`KIND`), where the spec
states it (`SPEC_PATH`) and the item's name (`NAME`): the run and its outcome
(`OUTCOME`), the input dataset read, the subjects built, each output column
with its missing values, and one row for the declared length of `AGE`. That
row names the column it checks (`TARGET`) and the rule behind the check
(`RULE`), and is a `warning` (`SEVERITY`) that the length understates the
ages (`OUTCOME` `violated`, with the condition in `CONDITION`): it counts
the two subjects whose age has two digits (`COUNT`) of the four checked
(`TOTAL`), names them by their keys (`KEYS`) and states the column and the
declared length (`DETAILS`).

To silence the warning, describe the age as wide as the ages the study can
collect, such as `length: 3` for an adult study; a collected age is never
changed to fit the description.
