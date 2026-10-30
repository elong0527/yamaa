"use strict";

// The review survey: drafts stay in this browser, and submitting opens
// GitHub's new-discussion composer with the answers filled in. The static page
// holds no credentials; the reviewer still selects Start discussion on GitHub.
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
  const suggestions = form.elements.suggestions;
  const feedback = form.querySelector(".review-feedback");
  const prepared = form.querySelector(".review-prepared");
  const preparedText = prepared.querySelector("textarea");

  function answer(fieldset) {
    const checked = fieldset.querySelector("input:checked");
    return checked ? checked.value : "";
  }

  function draft() {
    const values = { suggestions: suggestions.value };
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
    if (typeof saved.suggestions === "string") suggestions.value = saved.suggestions;
  } catch (error) { /* Start from an empty form. */ }

  function save() {
    try { window.localStorage.setItem(storageKey, JSON.stringify(draft())); }
    catch (error) { /* The draft lives only in the form. */ }
    prepared.hidden = true;
  }
  form.addEventListener("change", save);
  form.addEventListener("input", save);

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
    const text = suggestions.value.trim();
    return [
      "Review of the [" + name + "](" + page + ") benchmark.",
      "",
      "| Question | Answer |",
      "| --- | --- |",
      ...rows,
      "",
      "### Suggestions",
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
      show("Answer all three questions before submitting.", true);
      missing.querySelector("input").focus();
      return;
    }
    const title = "Review: " + name;
    const text = body();
    let url = composer(title, text);
    if (url.length > LINK_LIMIT) {
      url = composer(title, "");
      preparedText.value = text;
      prepared.hidden = false;
      show("Your review is longer than a prefilled link allows, so GitHub opens with the title only. Paste the text below into the discussion, then select Start discussion.");
    } else {
      show("GitHub opened in a new tab with your review filled in. Select Start discussion there to publish it.");
    }
    // A "noopener" feature makes window.open return null even when the tab
    // opens, so detach the opener by hand to tell a blocked tab apart.
    const tab = window.open(url, "_blank");
    if (tab) tab.opener = null;
    else show("Your browser blocked the new tab. Open GitHub from this link: " + url, true);
  });
})();
