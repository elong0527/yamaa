# ADaM ADEX: tell an uncollected dose from an absent administration

This example uses a subject-treatment inventory with its component exposure
records to derive one record per subject and treatment:

- `EXTRT` identifies the regimen component;
- `DOSECUM` is the total administered dose. It is empty when no dose was ever
  collected, so a quantity nobody recorded is never reported as a measured
  zero;
- `NDOSREC` is the number of administration records and `NDOSVAL` the number of
  those records carrying a dose. Both are empty when the component has no
  administration record at all.

A component whose doses were all left blank stays distinguishable from one that
was never administered: the first has administration records and no dose, and
counts them as such, while the second has nothing to count.
