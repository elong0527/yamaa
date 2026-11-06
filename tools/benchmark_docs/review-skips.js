"use strict";

// Skipping publishes nothing. The browser keeps each skip as a completion mark,
// and whatever the reviewer publishes next names the benchmarks skipped since
// in a Skipped row: a review, or the report the queue offers at its end. Once
// named, a skip keeps a mark the queue still treats as done. The review page
// and the dashboards both put this file before their own script.
const SKIPPED = "skipped";
const SKIP_REPORTED = "skip reported";

// The benchmarks skipped and not yet reported, other than `except`. Storage
// can be unavailable; then there is nothing to report.
function pendingSkips(doneKey, except) {
  try {
    const names = [];
    for (let index = 0; index < window.localStorage.length; index += 1) {
      const key = window.localStorage.key(index);
      const name = key.slice(doneKey.length);
      if (
        key.startsWith(doneKey) &&
        /^[a-z0-9-]+$/.test(name) &&
        name !== except &&
        window.localStorage.getItem(key) === SKIPPED
      ) names.push(name);
    }
    return names.sort();
  } catch (error) {
    return [];
  }
}

function skipsReported(doneKey, names) {
  try {
    names.forEach((name) => window.localStorage.setItem(doneKey + name, SKIP_REPORTED));
  } catch (error) { /* The skips are reported again next time. */ }
}

function skippedRow(names) {
  return "| **Skipped** | " + names.join(", ") + " |";
}
