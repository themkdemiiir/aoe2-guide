// Generic client-side list filter — static-safe (operates on pre-rendered DOM,
// no SSR, no data fetching). Re-initializes on Astro view-transition navigation.
//
// Markup contract:
//   [data-filter-root]                  wraps the filterable area
//     [data-filter-search]              <input> — matches against each item's data-search (substring)
//     [data-filter-facet="<name>"]      <select> — value filters items' data-<name> ("" = all)
//     [data-filter-reset]               optional button — clears search + facets
//     [data-filter-count]               element whose textContent becomes the visible count
//     [data-filter-empty]               element shown (unhidden) only when 0 items match
//     [data-filter-item]                each filterable item; carries data-search and data-<facet>
//                                       data-<facet> may be a space-separated list (e.g. data-civs="franks huns")

function initFilter(root: HTMLElement) {
  if (root.dataset.filterInit === "1") return; // guard against double-binding
  root.dataset.filterInit = "1";

  const search = root.querySelector<HTMLInputElement>("[data-filter-search]");
  const facets = Array.from(root.querySelectorAll<HTMLSelectElement>("[data-filter-facet]"));
  const items = Array.from(root.querySelectorAll<HTMLElement>("[data-filter-item]"));
  const countEl = root.querySelector<HTMLElement>("[data-filter-count]");
  const emptyEl = root.querySelector<HTMLElement>("[data-filter-empty]");

  function apply() {
    const q = (search?.value ?? "").trim().toLowerCase();
    const active = facets
      .map((f) => ({ name: f.dataset.filterFacet ?? "", val: f.value }))
      .filter((f) => f.name && f.val);

    let visible = 0;
    for (const item of items) {
      const haystack = item.dataset.search ?? "";
      let show = !q || haystack.includes(q);
      if (show) {
        for (const { name, val } of active) {
          const attr = item.getAttribute(`data-${name}`) ?? "";
          if (!attr.split(/\s+/).includes(val)) {
            show = false;
            break;
          }
        }
      }
      item.hidden = !show;
      if (show) visible++;
    }
    if (countEl) countEl.textContent = String(visible);
    if (emptyEl) emptyEl.hidden = visible !== 0;
  }

  search?.addEventListener("input", apply);
  for (const f of facets) f.addEventListener("change", apply);
  root.querySelector<HTMLElement>("[data-filter-reset]")?.addEventListener("click", () => {
    if (search) search.value = "";
    for (const f of facets) f.value = "";
    apply();
  });

  apply();
}

function initAll() {
  for (const root of document.querySelectorAll<HTMLElement>("[data-filter-root]")) {
    initFilter(root);
  }
}

// astro:page-load fires on first load AND after each view-transition navigation.
document.addEventListener("astro:page-load", initAll);
