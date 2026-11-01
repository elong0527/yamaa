"use strict";

// The review queue: sends a reviewer to the next benchmark they have neither
// submitted nor skipped. What they have done stays in this browser; `after`
// names the benchmark they just left, so the queue moves on without storage.
// Nobody assigns benchmarks, so each browser starts at its own place among the
// benchmarks nobody has reviewed, and reviewers do not all read the same ones.
(() => {
  const { doneKey, started, open } = JSON.parse(document.getElementById("review-queue").textContent);

  // Storage can be unavailable (private windows, blocked site data). Nothing
  // marks a benchmark done then, so the queue walks forward once and stops.
  function storage() {
    try {
      const probe = doneKey + "probe";
      window.localStorage.setItem(probe, "1");
      window.localStorage.removeItem(probe);
      return window.localStorage;
    } catch (error) {
      return null;
    }
  }

  function startingPlace(marks) {
    const key = doneKey + "start";
    const kept = marks.getItem(key);
    if (kept !== null) return Number(kept);
    const chosen = Math.random();
    marks.setItem(key, String(chosen));
    return chosen;
  }

  const marks = storage();
  const offset = marks ? Math.floor(startingPlace(marks) * open.length) : 0;
  const names = started.concat(open.slice(offset), open.slice(0, offset));
  const after = new URLSearchParams(window.location.search).get("after");
  const position = names.indexOf(after);
  const ahead = names.slice(position + 1);
  const order = marks ? ahead.concat(names.slice(0, position + 1)) : ahead;
  const next = order.find((name) => !marks || marks.getItem(doneKey + name) === null);
  if (next) {
    window.location.replace("../../" + encodeURIComponent(next) + ".html");
  } else {
    document.querySelector(".queue-status").hidden = true;
    document.querySelector(".queue-done").hidden = false;
  }
})();
