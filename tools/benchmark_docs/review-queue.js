"use strict";

// The review queue: sends a reviewer to the next benchmark they have neither
// submitted nor skipped. What they have done stays in this browser; `after`
// names the benchmark they just left, so the queue moves on without storage.
(() => {
  const { doneKey, names } = JSON.parse(document.getElementById("review-queue").textContent);

  // Storage can be unavailable (private windows, blocked site data).
  function done(name) {
    try { return window.localStorage.getItem(doneKey + name) !== null; }
    catch (error) { return false; }
  }

  const after = new URLSearchParams(window.location.search).get("after");
  const position = names.indexOf(after);
  const order = names.slice(position + 1).concat(names.slice(0, position + 1));
  const next = order.find((name) => !done(name));
  if (next) {
    window.location.replace("../" + encodeURIComponent(next) + ".html");
  } else {
    document.querySelector(".queue-status").hidden = true;
    document.querySelector(".queue-done").hidden = false;
  }
})();
