# One Collected Answer per Subject

**Goal:** derive `SEX` and `AGE` for one record per subject, each from the
one record that holds the subject's answer.

**Input:** long-form Operational Data Model (ODM) data with one row per
collected item, carrying the visit (`StudyEventOID`), the form, the item
(`ItemOID`), and the stored value (`Value`).

**Variables:**

- `SEX` is the sex answered at screening. A subject who is asked again at
  a later visit keeps the screening answer.
- `AGE` is the age collected for the subject; a subject with no age record
  keeps a missing age.

**Note:** an answer is read only when exactly one record holds it, so two
records of one question at one visit stop the run even when they agree.
Items the record does not read, such as a weight, play no part.
