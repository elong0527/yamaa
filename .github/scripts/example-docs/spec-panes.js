"use strict";

// Multi-specification pages only: switch the visible YAML pane.
// Single-specification pages never load this: without it every pane stays
// visible, so the page degrades to stacked docs.
(function () {
  const panes = Array.from(document.querySelectorAll(".spec-pane"));
  if (panes.length < 2) return;
  const aside = document.getElementById("specification");
  const scroller = aside.querySelector(".code-scroll");
  const toolbar = document.createElement("div");
  toolbar.className = "code-toolbar";
  const label = document.createElement("label");
  label.className = "visually-hidden";
  label.setAttribute("for", "spec-select");
  label.textContent = "Choose specification document";
  const select = document.createElement("select");
  select.id = "spec-select";
  panes.forEach((pane) => {
    const option = document.createElement("option");
    option.value = pane.id;
    option.textContent = pane.dataset.filename;
    select.append(option);
  });
  select.value = panes[panes.length - 1].id;
  toolbar.append(label);
  toolbar.append(select);
  aside.insertBefore(toolbar, scroller);
  const caption = aside.querySelector(".panel-caption");
  function showSpec(id) {
    panes.forEach((pane) => {
      pane.hidden = pane.id !== id;
    });
    const active = panes.find((pane) => pane.id === id) || panes[panes.length - 1];
    caption.textContent = active.dataset.lines + " lines";
  }
  select.addEventListener("change", () => showSpec(select.value));
  showSpec(select.value);
})();
