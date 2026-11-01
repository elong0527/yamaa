"use strict";

// The review survey: drafts stay in this browser, and submitting opens
// GitHub's new-discussion composer with the answers filled in. The static page
// holds no credentials; the reviewer still selects Start discussion on GitHub.
// Submitting or skipping then moves this page on through the review queue.
(() => {
  const form = document.querySelector(".review-form");
  if (!form) return;
  form.hidden = false;

  // GitHub documents no limit for prefilled links; browsers and proxies reject
  // very long ones, so a longer review opens with the title only.
  const LINK_LIMIT = 8000;
  const name = form.dataset.benchmark;
  const storageKey = "yamaa-benchmark-review:" + name;
  const questions = Array.from(form.querySelectorAll(".review-question"));
  const comments = form.elements.comments;
  const feedback = form.querySelector(".review-feedback");
  const prepared = form.querySelector(".review-prepared");
  const preparedText = prepared.querySelector("textarea");
  const storageUnavailable =
    new URLSearchParams(window.location.search).get("review-storage") === "unavailable";

  function answer(fieldset) {
    const checked = fieldset.querySelector("input:checked");
    return checked ? checked.value : "";
  }

  function approved() {
    return questions.every((fieldset) => answer(fieldset) === form.dataset.approve);
  }

  function draft() {
    const values = { comments: comments.value };
    questions.forEach((fieldset) => {
      values[fieldset.querySelector("input").name] = answer(fieldset);
    });
    return values;
  }

  // Storage can be unavailable (private windows, blocked site data); the form
  // still works without it.
  try {
    const saved = JSON.parse(window.localStorage.getItem(storageKey) || "{}");
    questions.forEach((fieldset) => {
      fieldset.querySelectorAll("input").forEach((input) => {
        input.checked = saved[input.name] === input.value;
      });
    });
    const text = typeof saved.comments === "string" ? saved.comments : saved.suggestions;
    if (typeof text === "string") comments.value = text;
  } catch (error) { /* Start from an empty form. */ }

  function save() {
    try { window.localStorage.setItem(storageKey, JSON.stringify(draft())); }
    catch (error) { /* The draft lives only in the form. */ }
    prepared.hidden = true;
  }
  form.addEventListener("change", save);
  form.addEventListener("input", save);

  // The queue reads this mark; without storage it moves on by position alone.
  function remember(outcome) {
    try {
      window.localStorage.setItem(form.dataset.doneKey + name, outcome);
      return true;
    } catch (error) {
      return false;
    }
  }

  function nextUrl(stored) {
    const url = new URL(form.dataset.nextUrl, window.location.href);
    if (storageUnavailable || !stored) url.searchParams.set("review-storage", "unavailable");
    return url.href;
  }

  function leave(outcome) {
    window.location.href = nextUrl(remember(outcome));
  }

  form.querySelector(".review-skip").addEventListener("click", () => leave("skipped"));

  function show(message, isError) {
    feedback.textContent = message;
    feedback.classList.toggle("is-error", Boolean(isError));
    feedback.hidden = false;
  }

  function cell(text) {
    return text.replace(/\|/g, "\\|").replace(/\s+/g, " ").trim();
  }

  function body() {
    const page = window.location.href.split("#")[0];
    const rows = questions.map((fieldset) => {
      const prompt = fieldset.querySelector(".review-prompt").textContent;
      return "| **" + cell(fieldset.dataset.label) + ":** " + cell(prompt) + " | " + cell(answer(fieldset)) + " |";
    });
    const text = comments.value.trim();
    return [
      "Review of the [" + name + "](" + page + ") benchmark.",
      "",
      "| Question | Answer |",
      "| --- | --- |",
      ...rows,
      "| **Overall** | " + (approved() ? "Approve" : "Not approve") + " |",
      "",
      "### Comments",
      "",
      text || "None.",
      "",
    ].join("\n");
  }

  function composer(title, text) {
    const url = new URL(form.dataset.reviewUrl);
    url.searchParams.set("title", title);
    if (text) url.searchParams.set("body", text);
    return url.href;
  }

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const missing = questions.find((fieldset) => !answer(fieldset));
    if (missing) {
      show("Answer every question before submitting.", true);
      missing.querySelector("input").focus();
      return;
    }
    if (!approved() && !comments.value.trim()) {
      show("Say why you disagree in your comments before submitting.", true);
      comments.focus();
      return;
    }
    const title = "Review: " + name;
    const text = body();
    let url = composer(title, text);
    const tooLong = url.length > LINK_LIMIT;
    if (tooLong) url = composer(title, "");
    // A "noopener" feature makes window.open return null even when the tab
    // opens, so detach the opener by hand to tell a blocked tab apart.
    const tab = window.open(url, "_blank");
    if (!tab) {
      show("Your browser blocked the new tab. Open GitHub from this link: " + url, true);
      return;
    }
    tab.opener = null;
    if (!tooLong) {
      leave("submitted");
      return;
    }
    // The text is still here to copy, so this page stays until it has been.
    prepared.querySelector("a").href = nextUrl(remember("submitted"));
    preparedText.value = text;
    prepared.hidden = false;
    show("Your review is longer than a prefilled link allows, so GitHub opened with the title only. Paste the text below into the discussion, select Start discussion, then go to the next benchmark.");
  });
})();
