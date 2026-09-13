"use strict";

document.body.classList.add("has-js");

const sidebarToggle = document.getElementById("sidebar-toggle");
const specification = document.getElementById("specification");
const workbench = document.querySelector(".workbench");
const specResizer = document.getElementById("spec-resizer");
sidebarToggle.hidden = false;
// The specification starts collapsed; the toggle reveals it on demand.
specification.hidden = true;
workbench.classList.add("without-sidebar");
sidebarToggle.setAttribute("aria-expanded", "false");
sidebarToggle.querySelector("span").textContent = "Show Spec";
sidebarToggle.addEventListener("click", () => {
  specification.hidden = !specification.hidden;
  workbench.classList.toggle("without-sidebar", specification.hidden);
  sidebarToggle.setAttribute("aria-expanded", String(!specification.hidden));
  sidebarToggle.querySelector("span").textContent = specification.hidden ? "Show Spec" : "Hide Spec";
});

const MIN_SPEC_WIDTH = 240;
const MAX_SPEC_WIDTH = 720;
const MIN_DATA_WIDTH = 420;

function specWidthBounds() {
  return {
    min: MIN_SPEC_WIDTH,
    max: Math.max(MIN_SPEC_WIDTH, Math.min(MAX_SPEC_WIDTH, workbench.clientWidth - MIN_DATA_WIDTH - specResizer.offsetWidth)),
  };
}

function setSpecWidth(width) {
  const bounds = specWidthBounds();
  const next = Math.round(Math.max(bounds.min, Math.min(bounds.max, width)));
  workbench.style.setProperty("--spec-width", next + "px");
  specResizer.setAttribute("aria-valuemin", String(bounds.min));
  specResizer.setAttribute("aria-valuemax", String(bounds.max));
  specResizer.setAttribute("aria-valuenow", String(next));
  specResizer.setAttribute("aria-valuetext", next + " pixels wide");
}

setSpecWidth(specification.getBoundingClientRect().width);

let resizing = false;
specResizer.addEventListener("pointerdown", (event) => {
  resizing = true;
  specResizer.setPointerCapture(event.pointerId);
  specResizer.classList.add("is-resizing");
  document.body.classList.add("is-resizing");
});
specResizer.addEventListener("pointermove", (event) => {
  if (!resizing) return;
  setSpecWidth(event.clientX - workbench.getBoundingClientRect().left);
});
function stopResize(event) {
  if (!resizing) return;
  resizing = false;
  if (specResizer.hasPointerCapture(event.pointerId)) specResizer.releasePointerCapture(event.pointerId);
  specResizer.classList.remove("is-resizing");
  document.body.classList.remove("is-resizing");
}
specResizer.addEventListener("pointerup", stopResize);
specResizer.addEventListener("pointercancel", stopResize);
specResizer.addEventListener("keydown", (event) => {
  const current = Number(specResizer.getAttribute("aria-valuenow"));
  const step = event.shiftKey ? 50 : 20;
  if (event.key === "ArrowLeft") setSpecWidth(current - step);
  else if (event.key === "ArrowRight") setSpecWidth(current + step);
  else if (event.key === "Home") setSpecWidth(specWidthBounds().min);
  else if (event.key === "End") setSpecWidth(specWidthBounds().max);
  else return;
  event.preventDefault();
});
window.addEventListener("resize", () => {
  if (!specification.hidden) {
    setSpecWidth(specification.getBoundingClientRect().width);
  }
});

const subjectSelect = document.getElementById("subject-select");
const subjectButtons = document.querySelectorAll(".subject-button");
if (subjectSelect.options.length > 1) {
  document.querySelector(".subject-control").hidden = false;
  function selectSubject(subject) {
    subjectSelect.value = subject;
    let inputRows = 0;
    let outputRows = 0;
    document.querySelectorAll("tr[data-subject]").forEach((row) => {
      const selected = subject !== "" && row.dataset.subject === subject;
      row.classList.toggle("is-selected", selected);
      if (selected && row.closest("#inputs")) inputRows += 1;
      if (selected && row.closest("#outputs")) outputRows += 1;
    });
    subjectButtons.forEach((button) => {
      button.setAttribute("aria-pressed", String(subject !== "" && button.dataset.subject === subject));
    });
    document.getElementById("selection-status").textContent = subject
      ? subjectSelect.selectedOptions[0].textContent + ": " + inputRows + " input rows and " + outputRows + " expected output rows highlighted across all files."
      : "All subjects shown. No rows highlighted.";
  }
  subjectSelect.addEventListener("change", () => selectSubject(subjectSelect.value));
  subjectButtons.forEach((button) => {
    button.addEventListener("click", () => selectSubject(button.dataset.subject === subjectSelect.value ? "" : button.dataset.subject));
  });
}

const codeSelect = document.getElementById("code-select");
if (codeSelect) {
  const codePanes = Array.from(document.querySelectorAll(".code-pane"));
  codeSelect.addEventListener("change", () => {
    codePanes.forEach((pane) => {
      pane.hidden = pane.id !== codeSelect.value;
    });
  });
}
