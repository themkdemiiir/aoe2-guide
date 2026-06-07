// Strips all AST nodes up to and including the boundary h2 in build-order
// markdown files so <Content /> renders only the unique strategy section.
// Boundary: "Strategy…" (EN) or "Strateji…" (TR) as the first text child.

function isBoundaryHeading(node) {
  return (
    node.type === "heading" &&
    node.depth === 2 &&
    node.children.some(
      (c) =>
        c.type === "text" &&
        (c.value.startsWith("Strategy") || c.value.startsWith("Strateji"))
    )
  );
}

function plugin(tree, vfile) {
  if (!vfile.path?.includes("/build-orders/")) return;
  const idx = tree.children.findIndex(isBoundaryHeading);
  if (idx >= 0) {
    tree.children.splice(0, idx + 1);
  }
}

export function remarkBuildOrderTruncate() {
  return plugin;
}
