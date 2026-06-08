// Header search modal — opens a Pagefind search dialog from the header button,
// the "/" key, or Cmd/Ctrl+K. Static-safe; re-wires on view-transition navigation.
// (Pagefind only returns results on the built site, not in `astro dev`.)

function openDialog(dialog: HTMLDialogElement) {
  if (dialog.open) return;
  dialog.showModal();
  // Focus the Pagefind search input once it renders.
  window.setTimeout(() => dialog.querySelector<HTMLInputElement>("input")?.focus(), 60);
}

function wire() {
  const trigger = document.getElementById("site-search-trigger");
  const dialog = document.getElementById("site-search") as HTMLDialogElement | null;
  if (!trigger || !dialog) return;

  if (!trigger.dataset.wired) {
    trigger.dataset.wired = "1";
    trigger.addEventListener("click", () => openDialog(dialog));
  }
  if (!dialog.dataset.wired) {
    dialog.dataset.wired = "1";
    // Click on the backdrop (the dialog element itself, outside its content) closes it.
    dialog.addEventListener("click", (e) => {
      if (e.target === dialog) dialog.close();
    });
  }
}

// Global shortcuts — bound once. This module executes a single time per session,
// and `document` survives view-transition navigations, so a module-scoped guard suffices.
let keysBound = false;
function bindKeys() {
  if (keysBound) return;
  keysBound = true;
  document.addEventListener("keydown", (e) => {
    const dialog = document.getElementById("site-search") as HTMLDialogElement | null;
    if (!dialog) return;
    const target = e.target as HTMLElement | null;
    const tag = target?.tagName ?? "";
    const typing = tag === "INPUT" || tag === "TEXTAREA" || target?.isContentEditable === true;
    const cmdK = (e.key === "k" || e.key === "K") && (e.metaKey || e.ctrlKey);
    const slash = e.key === "/" && !typing;
    if ((cmdK || slash) && !dialog.open) {
      e.preventDefault();
      openDialog(dialog);
    }
  });
  // Close the open mobile nav menu when clicking outside it.
  document.addEventListener("click", (e) => {
    const menu = document.querySelector("details[data-mobile-menu][open]");
    if (menu && !menu.contains(e.target as Node)) menu.removeAttribute("open");
  });
}

bindKeys();
document.addEventListener("astro:page-load", wire);
