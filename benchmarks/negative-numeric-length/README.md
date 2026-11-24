# Reject Ages Wider Than the Width Reviewers Are Told

**Goal:** carry each subject's age in whole years (`AGE`) into the
demographics domain.

**Input:** collected demographics with the study and subject identifiers
and the age in whole years, empty when it was not recorded.

**Variables:**

- `AGE` would be the collected age in years. It is described to reviewers
  as one digit wide, so an age of ten years or more contradicts that
  description. Every age is checked against it once the variable is filled
  in, and an age of two digits rejects the run, so no artifact is
  accepted. An age that was not recorded is left empty.

## How to fix

Describe the age as wide as the ages the study can collect. An adult study
records ages of up to three digits:

```yaml
submission:
  core: Exp
  length: 3
  origins:
    - type: Collected
      source: Investigator
```

Do not change a collected age to fit the description: `34` stays `34`, and
the declared `length` is what changes.
