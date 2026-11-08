---
title: Benchmark review
hide:
  - actions
---

# Benchmark review

<form class="review-start" action="next/" method="get">
  <label>Pair number (optional)
    <input name="pair" type="number" min="1" max="9999" step="1" inputmode="numeric">
  </label>
  <button class="md-button md-button--primary" type="submit">Start reviewing</button>
</form>

If you already started, this button takes you to your next benchmark. In a
session where you are asked to pair up, enter your pair's number: you and your
partner start at the same benchmark and give it the two reviews it needs.
Otherwise leave it blank.

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
| Reviewers | $reviewers |

Anyone can review. Your GitHub account appears here after your first review, and
the coordinator confirms the reviewers before a benchmark is finalized.

## How to review

We suggest spending no more than 5 minutes on a review. It gives a preliminary
approval, and the coordinator confirms the reviewers before a benchmark is
finalized.

1. Sign in to GitHub in your browser.
2. Select **Start reviewing**, after entering your pair number if you were
   given one.
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
stays approved and does not come back.

A review that does not approve opens a rework issue for the benchmark, with the
review's comments, so nobody needs to file one. The pull request that reworks
the benchmark closes the issue, and GitHub notifies the reviewer, who reviews
the benchmark again. The new review replaces their earlier one: agreeing can
approve the benchmark, and disagreeing opens another rework issue. When the
coordinator opens a new round, a benchmark that still needs rework starts it
with no reviews.

## Status

$status
