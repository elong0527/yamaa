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

Only the labels are governed. The rest is free text: nothing checks that a
length matches the variable it describes, that a named terminology list is the
one actually enforced on the values, or that a variable marked as derived is
one. Two studies can describe the same dataset in different words and both be
accepted, and no file is produced that a submission could carry, so none of it
is asserted anywhere.
