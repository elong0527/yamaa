# Reviewer feedback

Reviewers can report problems with yamaa's rules, examples, documentation, or
engines through the [Reviewer feedback form](https://github.com/elong0527/yamaa/issues/new?template=reviewer-feedback.yml).
GitHub issues, comments, Discussions, and pull requests in this repository are
public. Describe the behavior using information you are authorized to publish.
The form's acknowledgment is a reminder; it does not inspect submissions or
guarantee confidentiality.

## Prepare a public report

1. Start with the relevant public rule, REQ ID, documentation page, or
   repository benchmark. Describe the expected behavior and the observed
   behavior without identifying a sponsor, study, site, or participant.
2. For an engine problem, create the smallest example from scratch with
   invented values. Prefer an existing repository fixture when it demonstrates
   the same problem. Record the yamaa version, engine, and reproduction steps.
   Check that the invented example still reproduces the problem locally.
3. Review the entire report, including its title, code, pasted diagnostics,
   screenshots, attachments, filenames, local paths, and links. Remove patient
   and subject identifiers, dates and clinical records from real participants,
   confidential study or sponsor information, unpublished specifications and
   results, credentials, and internal URLs. Logs and document metadata can
   reveal information even when the visible example appears safe.
4. Confirm that your organization's review and disclosure requirements allow
   this report to be public. Submit the form only after completing its
   confidentiality acknowledgment. Apply the same check to later comments,
   attachments, and proposed fixes.

Replacing names or subject IDs in real records is not sufficient to make a
report safe: dates, combinations of values, and metadata can still identify
people or reveal confidential study information. Build synthetic examples
instead of uploading a real dataset, study specification, or screenshot.

## Example of useful feedback

For a data-processing problem, a report can contain:

```text
Area: Python engine
Reference: a public rule or benchmark path
Environment: yamaa version and Python version
Expected: two distinct synthetic subjects remain separate
Observed: describe the unexpected result using synthetic values
Steps: the smallest specification, invented input, and command that reproduce it
```

An invented input might be:

```csv
STUDYID,USUBJID,AGE
SYNTHETIC-STUDY,SYNTHETIC-001,30
SYNTHETIC-STUDY,SYNTHETIC-002,40
```

These values are invented, not transformed participant records. Include only
the columns and rows needed to demonstrate your finding. For a rule or
documentation question, a public link and a description of the ambiguity may
be enough; no data attachment is required.

## When the evidence must remain private

If you cannot reproduce or describe the finding safely with public information,
do not paste or upload the confidential evidence to GitHub. Use your
organization's existing approved internal review or incident-reporting process.
Ask the responsible study owner or privacy/security contact to decide what can
be shared and to help prepare a synthetic example.

yamaa does not currently designate a private intake channel for confidential
clinical or study data. A public issue may describe a general problem using
approved, non-sensitive information and explain that a public reproduction is
not yet available. Maintainers can discuss the behavior or request a synthetic
example there; do not supply private evidence in response to that request.

## If confidential information was posted

Stop adding or copying the information. Remove it from the public report where
you have permission, and follow your organization's incident-response process.
Contact [GitHub Support](https://support.github.com/) for assistance with exposed
content or attachments when needed. Editing or deleting a post does not
guarantee removal from notifications, revision history, caches, or copies. For
exposed credentials, follow the credential owner's revocation process as well.

Maintainers should avoid quoting exposed information or copying it into tests,
fixtures, commits, or CI artifacts. Request a synthetic reproduction and keep
public follow-up free of the confidential content.
