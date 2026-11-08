"use strict";

// The review queue: sends a reviewer to the next benchmark that still needs a
// review and that they have neither submitted nor skipped. What they have done
// stays in this browser; `after` names the benchmark they just left, so the
// queue moves on without storage.
//
// Nobody assigns benchmarks, and no benchmark goes to everyone first: each
// browser walks the benchmarks under review in gallery order from its own
// place, so people who start together read different benchmarks. The place is
// random, or comes from a pair number entered on the review page: everyone who
// enters the same number starts at the same benchmark, so the two people of a
// pair give it the two reviews it needs. A place is a share of the list of
// every benchmark under review, which a published review does not change, so
// partners who start minutes apart, on either side of a site rebuild, still
// start at the same benchmark.
(() => {
  const { doneKey, reviewUrl, benchmarks, unsettled } = JSON.parse(document.getElementById("review-queue").textContent);
  const waiting = new Set(unsettled);
  // Successive multiples of the golden ratio, modulo one, stay evenly spread
  // however many there are, so pair numbers need no head count.
  const GOLDEN = (Math.sqrt(5) - 1) / 2;

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

  // A pair number is a whole number from 1 to 9999; anything else is no number.
  function pairPlace(value) {
    return /^[1-9][0-9]{0,3}$/.test(value || "") ? (Number(value) * GOLDEN) % 1 : null;
  }

  // A pair number replaces the place this browser keeps, so a later visit
  // without one resumes from the pair's benchmark.
  function startingPlace(marks, pair) {
    const key = doneKey + "start";
    const kept = marks.getItem(key);
    if (pair === null && kept !== null) return Number(kept);
    const chosen = pair === null ? Math.random() : pair;
    try { marks.setItem(key, String(chosen)); }
    catch (error) { /* The place then changes on each visit. */ }
    return chosen;
  }

  const params = new URLSearchParams(window.location.search);
  const marks = params.get("review-storage") === "unavailable" ? null : storage();
  const pair = pairPlace(params.get("pair"));
  const place = marks ? startingPlace(marks, pair) : (pair === null ? 0 : pair);
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
