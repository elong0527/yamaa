# Take the effective AE state from a transaction log

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-ae-effective-transaction.html)

**Goal:** take the effective state of each adverse-event record:
the reported term (`AETERM`), severity (`AESEV`), and the kind and
time of the last change (`TXNTYPE` and `AUDITDTC`).

**Input:** a record list with one row per subject and event, plus a
transaction log with one row per change carrying the sequence
number, change kind, audit time, reported term, and severity.

**Variables:**

- `AETERM` carries the reported term from the effective change.
- `AESEV` carries the severity from the effective change.
- `TXNTYPE` carries the kind of the effective change: `INSERT` or
  `UPDATE`.
- `AUDITDTC` carries the time of the effective change.

**Note:** the effective change is the one with the latest audit
time; when two changes share that time the higher sequence number
wins, so timestamp order decides even when it disagrees with log
position. A record whose effective change is `REMOVE` is dropped
instead of kept.

**Standard:** SDTM | **Domain:** AE
