---
title: Benchmark review
hide:
  - actions
---

# Benchmark review

<a class="md-button md-button--primary" href="next/">Start reviewing</a>

If you already started, this button takes you to your next benchmark.

## Purpose

Benchmarks are the worked examples and conformance tests of yamaa. This review
asks clinical trial statisticians and programmers to confirm that each `sdtm`
and `adam` benchmark represents real work before it is finalized. It covers
$total. Comments from before this review do not count.

## People

| Role | Who |
|---|---|
| Owner | `elong0527` |
| Coordinator | `pzhang724` |

Anyone can review. Your GitHub account appears in the table of reviewers below
after your first review, and the coordinator confirms the reviewers before a
benchmark is finalized.

The table counts what each reviewer has contributed in all rounds: the
benchmarks they reviewed, and the reviews they published, by decision. A new
review of the same benchmark after its rework counts as one more review, not
one more benchmark. Thank you for every review!

$reviewers

## How to review

We suggest spending no more than 5 minutes on a review. It gives a preliminary
approval, and the coordinator confirms the reviewers before a benchmark is
finalized.

1. Sign in to GitHub in your browser.
2. Select **Start reviewing**. The first time, enter your GitHub username, so
   the queue does not offer you a benchmark you have already reviewed in this
   round, in any browser.
3. Read the Summary, Inputs and Expected output.
4. Answer each question with Agree or Disagree: Task, Inputs, and, where
   shown, the AI-assisted review of three potential concerns. If you disagree,
   say why in the comments.
5. Select **Submit and next**. In the GitHub tab, tick the search box if GitHub
   shows it, select **Start discussion**, then close the tab.
6. Select **Skip** for any benchmark you cannot judge. Your next review lists
   the benchmarks you skipped, so the status below can count them. At the end
   of the queue you can publish any skips that are left.

Reviews are public, so use public or invented data only.

A benchmark passes when two reviewers approve it and nobody disagrees. It then
stays approved and does not come back. The next section says what happens to
a review after you submit it.

## How feedback is handled

Each review is feedback on one benchmark. The feedback goes through these
steps until the benchmark passes.

### 1. Ask for reviews

The coordinator shares this page with clinical trial statisticians and
programmers. Nobody assigns benchmarks. **Start reviewing** gives each reviewer
a benchmark that still needs reviews in this round and that they have not
reviewed yet.

### 2. Share the feedback

Each review is a public discussion named `Review: <benchmark>` in the
[Discussions](https://github.com/elong0527/yamaa/discussions) of the
repository. The [Status](#status) table below shows each decision as soon as
GitHub publishes it, and each decision links to its review. To discuss a
disagreement, filter the table, for example to the decisions that do not
approve, and share the page address.

A review that does not approve opens a
[rework issue](https://github.com/elong0527/yamaa/issues?q=label%3Abenchmark-review)
right away. The issue quotes the review and mentions the reviewer, so nobody
needs to file it. It also states whether the reviewer is confirmed: the review
of an account new to the repository is labelled `needs-human-input`, and the
coordinator reads it before any work starts.

### 3. Improve the benchmark

The coordinator reads each rework issue and decides what to do. Comments that
share one cause are handled together.

- When the benchmark should change, a pull request changes it as the review
  asks, for example its specification, inputs, expected output, summary or
  potential concerns. The expected output is written by hand from the inputs.
  It is never copied from what yamaa produces. The pull request says which
  expected values changed and why, and it closes the rework issue. The owner
  reviews and merges the pull request.
- When a comment is about how this site shows a benchmark, the pull request
  changes the site instead.
- When the benchmark should stay as it is, the coordinator answers on the
  review, says why, and closes the rework issue as not planned.

### 4. Tell the reviewer

The reviewer hears back through their GitHub notifications:

1. When the pull request merges, it closes the rework issue. GitHub notifies
   the reviewer, because the issue mentions them.
2. After the site has updated, the coordinator comments on the rework issue
   and on the review, and mentions the reviewer. The comment says that the
   change is live and asks for a new review.

When the benchmark stays as it is, the coordinator's answer on the review
mentions the reviewer instead.

### 5. Review the same benchmark again

1. Open the benchmark from the link in the comment. **Start reviewing** does
   not offer it to you again in the same round, because you have already
   reviewed it.
2. Read what changed, answer the questions again, and select
   **Submit and next**. This publishes a new review.

Do not edit your earlier review, and do not reply to it, because a reply does
not count as a review. Your new review replaces your earlier one. Agreeing can
approve the benchmark, and disagreeing opens another rework issue.

### 6. Track progress to the goal

The goal is that every benchmark under review passes and is finalized.

This page shows how far the review has come. It updates a few minutes after a
review is published, edited or deleted.

- [Status](#status) counts the benchmarks in each status, by round and by
  group, and lists each benchmark with its decisions.
- [People](#people) counts the benchmarks and reviews of each reviewer.
- The open
  [rework issues](https://github.com/elong0527/yamaa/issues?q=is%3Aopen+label%3Abenchmark-review)
  are the feedback that still needs an answer.

A round ends when the rework of its benchmarks is done. The coordinator then
opens a new round. A benchmark that still needs rework starts the new round
with no reviews, so **Start reviewing** offers it to everyone again. Other
reviews carry over.

For a question about the review, mention the coordinator, `pzhang724`, on your
review or on its rework issue.

## Status

$status
