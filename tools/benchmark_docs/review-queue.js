"use strict";

// The review queue: sends a reviewer to the next benchmark that still needs a
// review and that they have neither submitted nor skipped. What they have done
// stays in this browser; `after` names the benchmark they just left, so the
// queue moves on without storage.
//
// Nobody assigns benchmarks, and no benchmark goes to everyone first: each
// browser walks the benchmarks under review in gallery order from its own
// random place, so people who start together read different benchmarks. The
// place is a share of the list of every benchmark under review, which a
// published review does not change, so a site rebuild does not move it.
(() => {
  const { doneKey, reviewUrl, benchmarks, unsettled } = JSON.parse(document.getElementById("review-queue").textContent);
  const waiting = new Set(unsettled);

  // Storage can be unavailable (private windows, blocked site data). Nothing
  // marks a benchmark done then, so the queue walks forward once and stops.
  function storage() {
    try {
      const probe = doneKey + "probe";
      window.localStorage.setItem(probe, "1");
      const readable = window.localStorage.getItem(probe) === "1";
      window.localStorage.removeItem(probe);
      return readable ? window.localStorage : null;
    } catch (error) {
      return null;
    }
  }

  function startingPlace(marks) {
    const key = doneKey + "start";
    const kept = marks.getItem(key);
    if (kept !== null) return Number(kept);
    const chosen = Math.random();
    try { marks.setItem(key, String(chosen)); }
    catch (error) { /* The place then changes on each visit. */ }
    return chosen;
  }

  const params = new URLSearchParams(window.location.search);
  const marks = params.get("review-storage") === "unavailable" ? null : storage();
  const place = marks ? startingPlace(marks) : 0;
  const after = params.get("after");
  const position = benchmarks.indexOf(after);
  const start = position >= 0 ? position + 1 : Math.floor(place * benchmarks.length);
  const ahead = benchmarks.slice(start);
  const order = marks ? ahead.concat(benchmarks.slice(0, start)) : ahead;
  const next = order.find(
    (name) => waiting.has(name) && name !== after && (!marks || marks.getItem(doneKey + name) === null),
  );
  if (next) {
    const url = new URL("../../" + encodeURIComponent(next) + ".html", window.location.href);
    if (!marks) url.searchParams.set("review-storage", "unavailable");
    window.location.replace(url.href);
  } else {
    document.querySelector(".queue-status").hidden = true;
    document.querySelector(".queue-done").hidden = false;
    // No later review will name the skips left, so offer to publish them.
    const skipped = marks ? pendingSkips(doneKey, null) : [];
    if (skipped.length) {
      const report = document.querySelector(".queue-skips");
      const link = report.querySelector("a");
      const url = new URL(reviewUrl);
      url.searchParams.set("title", "Review: skipped benchmarks");
      url.searchParams.set("body", [
        "Benchmarks I skipped in the review because I could not judge them.",
        "",
        "| Review | Benchmarks |",
        "| --- | --- |",
        skippedRow(skipped),
        "",
      ].join("\n"));
      link.href = url.href;
      link.addEventListener("click", () => skipsReported(doneKey, skipped));
      report.querySelector(".queue-skip-count").textContent =
        skipped.length === 1 ? "1 benchmark" : skipped.length + " benchmarks";
      report.hidden = false;
    }
  }
})();
