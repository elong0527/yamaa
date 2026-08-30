# SDTM DM: declare the metadata a submission needs

This example uses collected DM data and a `yamaa` specification to derive one
record per subject. `SITEID`, `AGE`, `AGEU`, `SEX`, and `COUNTRY` are collected
values copied through, and `USUBJID` is built from study, site, and subject
number. The derivations are deliberately ordinary so that the subject of the
example is the metadata declared around them rather than the values
themselves.

Every variable declares the label a reviewer sees, where its value came from,
how long it can be, and, where one applies, the controlled terminology list it
draws on. The dataset itself declares its label, its class, its structure, and
the standard version it follows.

The labels are governed, and so is how long a `USUBJID` may be: a subject
identifier longer than the study permits stops the run rather than being
shortened to fit. The rest is free text. The lengths recorded beside the other
variables are not connected to their values, nothing checks that a named
terminology list is the one actually enforced on the values, and nothing
checks that a variable marked as derived is one. Two studies can describe the
same dataset in different words and both be accepted, and no file is produced
that a submission could carry, so most of it is asserted nowhere.
