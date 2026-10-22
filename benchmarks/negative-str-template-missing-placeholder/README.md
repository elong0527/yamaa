# Reject a Subject Identifier Built Without Its Site

**Goal:** build the unique subject identifier (`USUBJID`) from the study,
the site and the subject number, beside the sex (`SEX`).

**Input:** collected demographics with one record per subject; one
subject's site was not recorded.

**Variables:**

- `USUBJID` would be the study, the site and the subject number joined by
  hyphens, such as `CTX-701-0001`.
- `SUBJID`, `SITEID` and `SEX` are the subject number, the site and the sex
  as collected.

Without its site, the second subject's identifier would be incomplete.
Leaving the site out, or writing a stand-in for it, would give an
identifier that could clash with another subject's or name no site at all.
The run is rejected, and no artifact is accepted.

## How to fix

Find the site the subject enrolled at in the source, and export the record
again with it:

    STUDYID,SITEID,SUBJID,SEX
    CTX,701,0001,F
    CTX,703,0002,M
    CTX,702,0003,F

Do not invent a site for the identifier: it is only as reliable as the
values it is built from.
