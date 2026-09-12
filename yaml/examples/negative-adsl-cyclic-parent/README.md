# ADaM ADSL: reject a circular chain of shared definitions

This example attempts to prepare subject records from a reusable file that
eventually points back to the requested file.

Following the chain would never reach a stable set of definitions, so the run
must fail before any source data is read.

## How to fix

Remove the backward `parents` reference so that every path through the chain
terminates. Keep genuinely shared definitions in one common ancestor.
