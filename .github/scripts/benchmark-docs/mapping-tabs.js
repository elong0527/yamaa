"use strict";

// Tabbed "Derived Excel Specification" section: switch the visible sheet pane.
// Without this script every pane stays visible, so the section degrades to
// stacked tables.
(function () {
  const section = document.getElementById("mapping-spec");
  if (!section) return;
  const tabs = Array.from(section.querySelectorAll('[role="tab"]'));
  const panes = Array.from(section.querySelectorAll('[role="tabpanel"]'));
  if (tabs.length < 2) return;

  function select(tab, focus) {
    tabs.forEach((other) => {
      const active = other === tab;
      other.setAttribute("aria-selected", active ? "true" : "false");
      other.tabIndex = active ? 0 : -1;
      const pane = document.getElementById(other.getAttribute("aria-controls"));
      if (pane) pane.hidden = !active;
      if (active && focus) other.focus();
    });
  }

  tabs.forEach((tab, index) => {
    tab.addEventListener("click", () => select(tab, false));
    tab.addEventListener("keydown", (event) => {
      let next = null;
      if (event.key === "ArrowRight") next = tabs[(index + 1) % tabs.length];
      else if (event.key === "ArrowLeft") next = tabs[(index - 1 + tabs.length) % tabs.length];
      else if (event.key === "Home") next = tabs[0];
      else if (event.key === "End") next = tabs[tabs.length - 1];
      if (next) {
        event.preventDefault();
        select(next, true);
      }
    });
  });

  select(tabs[0], false);
})();
