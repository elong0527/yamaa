# Reject a Site Entered Without Its Site Number

**Goal:** take each subject's site number (`SITEID`) from the collected site
description.

**Input:** collected demographics whose `SITE` text gives the site number
and the city, such as `Site 701 - Copenhagen`; one subject's site was
entered by its name alone.

**Variables:**

- `SITEID` would be the three-digit site number taken from `SITE`.

`Karolinska Stockholm` holds no site number, and nothing in the study says
which number that site has, so the subject's site cannot be told. Leaving
the site empty would lose a collected fact. The run is rejected, and no
artifact is accepted.

## How to fix

Correct the entry at the source so that it gives the site number:

    STUDYID,USUBJID,SITE
    CTX,CTX-73,Site 703 - Stockholm

If some sites are known only by name, state how the study numbers them,
for example by looking each name up in a list of sites, rather than leaving
their site empty.
