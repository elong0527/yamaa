"use strict";

// The panel under the datasets: Comments, then the assessment materials. A
// tab shows its pane alone; with "Side by side" on, tabs toggle and up to
// three panes share the row, in tab order. A pane holding several documents
// shows the one its picker names. Without this script the tabs and the switch
// stay hidden and every pane and document is shown, stacked.
(function () {
  const panel = document.getElementById("discussion");
  if (!panel) return;
  const tablist = panel.querySelector('[role="tablist"]');
  const tabs = Array.from(tablist.querySelectorAll('[role="tab"]'));
  const grid = panel.querySelector(".pane-grid");
  const toggle = document.getElementById("split-toggle");
  const LIMIT = 3;
  const paneOf = (tab) => document.getElementById(tab.getAttribute("aria-controls"));
  let open = [tabs[0]];

  panel.querySelectorAll("select[data-documents]").forEach((select) => {
    const documents = Array.from(select.options, (option) => document.getElementById(option.value));
    const show = () => documents.forEach((item) => {
      item.hidden = item.id !== select.value;
    });
    select.addEventListener("change", show);
    show();
  });
  if (tabs.length < 2) return;
  panel.classList.add("is-tabbed");
  tablist.hidden = false;
  toggle.closest("label").hidden = false;

  // Mirror `open` onto the tabs, the panes, and the side-by-side layout.
  function render() {
    const split = toggle.checked;
    tablist.setAttribute("aria-multiselectable", String(split));
    panel.classList.toggle("is-split", split);
    grid.style.setProperty("--open-panes", String(open.length));
    tabs.forEach((tab) => {
      const active = open.includes(tab);
      tab.setAttribute("aria-selected", String(active));
      tab.tabIndex = split || active ? 0 : -1;
      paneOf(tab).hidden = !active;
    });
  }

  // One pane at a time: show this one. Side by side: toggle it, keeping at
  // least one open and dropping the oldest past the limit.
  function choose(tab) {
    if (!toggle.checked) open = [tab];
    else if (open.includes(tab)) {
      if (open.length > 1) open = open.filter((other) => other !== tab);
    } else {
      open = open.concat(tab).slice(-LIMIT);
    }
    render();
  }

  tabs.forEach((tab, index) => {
    tab.addEventListener("click", () => choose(tab));
    tab.addEventListener("keydown", (event) => {
      let next = null;
      if (event.key === "ArrowRight") next = tabs[(index + 1) % tabs.length];
      else if (event.key === "ArrowLeft") next = tabs[(index - 1 + tabs.length) % tabs.length];
      else if (event.key === "Home") next = tabs[0];
      else if (event.key === "End") next = tabs[tabs.length - 1];
      if (!next) return;
      event.preventDefault();
      next.focus();
      // One pane at a time follows the focus; side by side, Enter or Space toggles.
      if (!toggle.checked) choose(next);
    });
  });
  toggle.addEventListener("change", () => {
    if (!toggle.checked) open = open.slice(-1);
    render();
  });

  // A link into the panel (#comments, #code, a code line) opens the tab and
  // the document that hold it.
  function reveal() {
    const target = location.hash && document.getElementById(decodeURIComponent(location.hash.slice(1)));
    if (!target || !grid.contains(target)) return false;
    const tab = tabs.find((item) => paneOf(item).contains(target));
    const item = target.closest(".material-document");
    const select = item && paneOf(tab).querySelector("select[data-documents]");
    let changed = false;
    if (select && select.value !== item.id) {
      select.value = item.id;
      select.dispatchEvent(new Event("change"));
      changed = true;
    }
    if (!open.includes(tab)) {
      choose(tab);
      changed = true;
    }
    return changed;
  }
  window.addEventListener("hashchange", () => {
    if (reveal()) document.getElementById(decodeURIComponent(location.hash.slice(1))).scrollIntoView();
  });
  render();
  reveal();
})();
