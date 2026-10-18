# Derived Demographics Stay Blank When the Collected Value Is

**Goal:** derive the site (`SITEID`), the age group (`AGEGR1`), the numeric
sex (`SEXN`), the race (`RACE`) and the country (`COUNTRY`) from collected
demographics, each left blank when the collected value it reads is.

**Input:** one `spec.yaml` over collected demographics carrying a site code
written `SITE-` and a number, the age, the sex code, the race with its words
joined by underscores, and a lower-case country code. Some subjects lack one
of these values.

**Variables:**

- `SITEID` is the number in the collected site code; a subject with no site
  code has no site.
- `AGEGR1` is `<65` below 65 years and `>=65` from 65 years on; a subject with
  no recorded age has no age group.
- `SEXN` is 1 for male and 2 for female; a subject with no recorded sex has
  no numeric sex.
- `RACE` is the collected race with each underscore read as a space; a subject
  with no recorded race has none.
- `COUNTRY` is the collected country code in upper case; a subject with no
  recorded country has none.

**Note:** a missing collected value leaves the value derived from it blank
instead of stopping the run, nothing is filled in for it, and each such blank
is counted so the run reports how often it happened.
