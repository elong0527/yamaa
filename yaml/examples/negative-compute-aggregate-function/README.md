# ADaM ADLB: reject a total written as a formula

This example uses collected laboratory results to attempt one record per
subject and parameter:

- `AVAL` is the collected result;
- `AVALTOT` is meant to total the results across the records of a subject.

A formula describes one record at a time and cannot reach the other records it
would have to total, and the specification never says which records belong to
the total. Reading the name as a total over some assumed set of records would
answer a question nobody asked, so the run must fail and no artifact is
accepted.
