# ADaM ADSL: reject shared definitions from another language version

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adsl-parent-version-mismatch.html)

This example attempts to prepare subject records from definitions written for
a different version of the language.

Combining meanings from two versions could silently reinterpret the requested
dataset, so the run must fail before any source data is read.

## How to fix

Migrate the parent file and the entry file together, then give every layer the
same `schema_version` as the active bundle.
