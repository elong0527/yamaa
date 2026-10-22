# Reject a Birth Date Before 1582 in SAS Transport

**Goal:** carry the date of birth (`BRTHDT`) into ADSL and submit it as the
SAS transport file `adsl.xpt`, in which an analysis date is a SAS date.

**Input:** one pre-analysis row per subject carrying the collected date of
birth; one was entered as 12 June 1498 instead of 1948.

**Variables:**

- `BRTHDT` is the subject's date of birth as collected.

A SAS date counts days from 1 January 1960, and SAS dates begin on
1 January 1582, so a transport file cannot carry 1498-06-12. Writing any
other date in its place would change a recorded value, so the submission is
rejected before any file is written.

## How to fix

Query the site and correct the year at the source, then build the dataset
again:

    STUDYID,USUBJID,BRTHDTC
    CATH,CATH-UCSD-0001,1958-03-14
    CATH,CATH-UCSD-0002,1948-06-12
    CATH,CATH-MCGILL-0003,1972-11-30
