// Canonical HTML-escape for strings interpolated into innerHTML.
// is:inline scripts can't import this — their local copies must carry a
// "// mirror of src/scripts/html.ts" comment and be kept in sync.
export const esc = (s: unknown) => String(s ?? "").replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
