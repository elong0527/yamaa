"use strict";

// Multi-specification pages only: switch the visible YAML pane.
// Single-specification pages never load this: without it every pane stays
// visible, so the page degrades to stacked docs.
(function () {
  const panes = Array.from(document.querySelectorAll(".spec-pane"));
  if (panes.length < 2) return;
  const aside = document.getElementById("specification");
  const scroller = aside.querySelector(".code-scroll");
  const fileHeading = document.createElement("div");
  fileHeading.className = "file-heading spec-file-heading";
  const fileTitle = document.createElement("span");
  fileTitle.className = "file-title";
  const label = document.createElement("label");
  label.className = "visually-hidden";
  label.setAttribute("for", "spec-select");
  label.textContent = "Choose specification document";
  const select = document.createElement("select");
  select.id = "spec-select";
  select.setAttribute("aria-label", "Specification path");
  panes.forEach((pane) => {
    const option = document.createElement("option");
    option.value = pane.id;
    option.textContent = pane.dataset.filename;
    select.append(option);
  });
  select.value = panes[panes.length - 1].id;
  const edit = document.createElement("a");
  edit.className = "edit-button";
  edit.textContent = "Edit";
  const count = document.createElement("span");
  count.className = "file-count";
  fileTitle.append(label, select, edit);
  fileHeading.append(fileTitle, count);
  scroller.before(fileHeading);
  aside.classList.add("has-spec-picker");
  function showSpec(id) {
    panes.forEach((pane) => {
      pane.hidden = pane.id !== id;
    });
    const active = panes.find((pane) => pane.id === id) || panes[panes.length - 1];
    edit.hidden = !active.dataset.editUrl;
    if (active.dataset.editUrl) {
      edit.href = active.dataset.editUrl;
      edit.setAttribute("aria-label", "Edit " + active.dataset.filename);
    } else {
      edit.removeAttribute("href");
      edit.removeAttribute("aria-label");
    }
    count.textContent = active.dataset.lines + " lines";
    scroller.setAttribute("aria-label", "Complete " + active.dataset.filename + " specification");
  }
  select.addEventListener("change", () => showSpec(select.value));
  showSpec(select.value);
})();
