# ADaM ADSL: reject shared definitions from a remote location

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adsl-remote-parent.html)

This example attempts to prepare subject records from definitions named by a
web address.

A remote resource can change independently and cannot provide a reproducible
local build, so the run must fail before any source data is read.

## How to fix

Review and store the parent file locally, then reference it with a relative or
absolute filesystem `parents` path. Do not use a URL or URI.
