"use strict";

// Filters the status table of the review page, so reviewers find the
// benchmarks they need to align on, such as every adam benchmark that needs
// rework. Each control is named after a data attribute of the rows: a choice
// must equal it, and typed text must occur in it. The address keeps the
// filters, so a filtered table can be shared as a link.
(() => {
  const form = document.querySelector(".review-filter");
  if (!form) return;
  const controls = Array.from(form.querySelectorAll("[name]"));
  const rows = Array.from(document.querySelectorAll(".review-status tbody tr"));
  const count = document.querySelector(".review-filter-count");
  const url = new URL(window.location.href);
  for (const control of controls) {
    const wanted = url.searchParams.get(control.name) ?? "";
    control.value = wanted;
    // A choice the table no longer offers selects nothing; show every row.
    if (control.value !== wanted) control.value = "";
  }

  function matches(row, control) {
    const wanted = control.value.trim().toLowerCase();
    const value = row.dataset[control.name].toLowerCase();
    return control.tagName === "SELECT" ? !wanted || value === wanted : value.includes(wanted);
  }

  function apply() {
    let shown = 0;
    for (const row of rows) {
      row.hidden = !controls.every((control) => matches(row, control));
      if (!row.hidden) shown += 1;
    }
    count.textContent = `Showing ${shown} of ${rows.length} benchmarks.`;
    for (const control of controls) {
      if (control.value) url.searchParams.set(control.name, control.value);
      else url.searchParams.delete(control.name);
    }
    window.history.replaceState(window.history.state, "", url.href);
  }

  form.addEventListener("input", apply);
  form.addEventListener("submit", (event) => event.preventDefault());
  // The reset event fires before the controls clear.
  form.addEventListener("reset", () => window.setTimeout(apply));
  form.hidden = false;
  count.hidden = false;
  apply();
})();
