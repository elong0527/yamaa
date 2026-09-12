"use strict";

// Multi-specification pages only: switch the visible YAML pane and rebuild
// the section jump list from it. Single-specification pages never load this:
// without it every pane stays visible, so the page degrades to stacked docs.
(function () {
  const panes = Array.from(document.querySelectorAll(".spec-pane"));
  if (panes.length < 2) return;
  const toolbar = document.querySelector(".code-toolbar");
  const sectionSelect = document.getElementById("section-select");
  const caption = document.querySelector("#specification .panel-caption");
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
  toolbar.prepend(select);
  toolbar.prepend(label);
  function showSpec(id) {
    panes.forEach((pane) => {
      pane.hidden = pane.id !== id;
    });
    const active = panes.find((pane) => pane.id === id) || panes[panes.length - 1];
    caption.textContent = active.dataset.lines + " lines";
    sectionSelect.innerHTML = '<option value="">Jump to section</option>';
    active.querySelectorAll(".code-line").forEach((line) => {
      const source = line.querySelector(".code-source");
      const match = source ? source.textContent.match(/^([A-Za-z_][\w-]*):/) : null;
      if (match) {
        const option = document.createElement("option");
        option.value = line.id;
        option.textContent = match[1];
        sectionSelect.append(option);
      }
    });
  }
  select.addEventListener("change", () => showSpec(select.value));
  showSpec(select.value);
})();
