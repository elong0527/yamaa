"use strict";

// The review queue: sends a reviewer to the next benchmark that still needs a
// review and that they have neither submitted nor skipped. What they have done
// stays in this browser; `after` names the benchmark they just left, so the
// queue moves on without storage. The published reviews of this round name
// their reviewers too, so the queue asks for the reviewer's GitHub username
// once and never offers them a benchmark they reviewed in this round, in any
// browser.
//
// Nobody assigns benchmarks, and no benchmark goes to everyone first: each
// browser walks the benchmarks under review in gallery order from its own
// random place, so people who start together read different benchmarks. The
// place is a share of the list of every benchmark under review, which a
// published review does not change, so a site rebuild does not move it.
(() => {
  const { doneKey, reviewUrl, benchmarks, unsettled } = JSON.parse(document.getElementById("review-queue").textContent);
  const REVIEWER_KEY = "yamaa-benchmark-reviewer";
  // GitHub usernames: letters, digits and hyphens, not starting with a hyphen.
  const USERNAME = /^[A-Za-z0-9][A-Za-z0-9-]{0,38}$/;

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

  function username(text) {
    const name = (text || "").trim().replace(/^@/, "");
    return USERNAME.test(name) ? name : null;
  }

  // Whether storage keeps the username; one it cannot keep stays in the
  // address, where the review form hands it back to the queue.
  function keep(name, marks) {
    if (!marks) return false;
    try {
      marks.setItem(REVIEWER_KEY, name);
      return true;
    } catch (error) {
      return false;
    }
  }

  // A plain form that reloads this page with the answer and every other
  // parameter it came with.
  function askReviewer(params) {
    document.querySelector(".queue-status").hidden = true;
    const form = document.querySelector(".queue-reviewer");
    params.forEach((value, key) => {
      if (key === "reviewer") return;
      const field = document.createElement("input");
      field.type = "hidden";
      field.name = key;
      field.value = value;
      form.append(field);
    });
    form.querySelector(".queue-reviewer-retry").hidden = !params.has("reviewer");
    form.hidden = false;
    form.elements.reviewer.focus();
  }

  const params = new URLSearchParams(window.location.search);
  const marks = params.get("review-storage") === "unavailable" ? null : storage();
  // The address names the reviewer when they answer the question below.
  const given = username(params.get("reviewer"));
  const reviewer = given || (marks ? username(marks.getItem(REVIEWER_KEY)) : null);
  if (!reviewer) {
    askReviewer(params);
    return;
  }
  const carried = given !== null && !keep(given, marks);
  // GitHub compares usernames without case.
  const me = reviewer.toLowerCase();
  const waiting = new Set(
    Object.keys(unsettled).filter(
      (name) => !unsettled[name].some((login) => login.toLowerCase() === me),
    ),
  );
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
    if (carried) url.searchParams.set("reviewer", reviewer);
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
