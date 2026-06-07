import { describe, it, expect } from "vitest";
import { remarkBuildOrderTruncate } from "./build-order-truncate.mjs";

const plugin = remarkBuildOrderTruncate();

type TextNode = { type: "text"; value: string };
type HeadingNode = { type: "heading"; depth: number; children: TextNode[] };
type ParagraphNode = { type: "paragraph"; children: TextNode[] };
type AstNode = HeadingNode | ParagraphNode;

function makeTree(children: AstNode[]) {
  return { type: "root", children };
}

describe("remarkBuildOrderTruncate", () => {
  it("strips pre-strategy content (inclusive) in EN build-order files", () => {
    const tree = makeTree([
      { type: "heading", depth: 1, children: [{ type: "text", value: "Title" }] },
      { type: "heading", depth: 2, children: [{ type: "text", value: "Dark Age (0-18)" }] },
      { type: "paragraph", children: [{ type: "text", value: "Steps..." }] },
      {
        type: "heading",
        depth: 2,
        children: [{ type: "text", value: "Strategy and Follow-Up (What's Next)" }],
      },
      { type: "paragraph", children: [{ type: "text", value: "The real strategy." }] },
    ]);

    plugin(tree as any, { path: "/src/content/build-orders/en/18pop-scouts.md" } as any);

    expect(tree.children).toHaveLength(1);
    expect(tree.children[0].type).toBe("paragraph");
    expect((tree.children[0] as ParagraphNode).children[0].value).toBe("The real strategy.");
  });

  it("strips pre-strategy content (inclusive) in TR build-order files", () => {
    const tree = makeTree([
      { type: "heading", depth: 1, children: [{ type: "text", value: "Başlık" }] },
      { type: "heading", depth: 2, children: [{ type: "text", value: "Karanlık Çağ" }] },
      { type: "paragraph", children: [{ type: "text", value: "Adımlar..." }] },
      {
        type: "heading",
        depth: 2,
        children: [{ type: "text", value: "Strateji ve Devam (What's Next)" }],
      },
      { type: "paragraph", children: [{ type: "text", value: "Asıl strateji." }] },
    ]);

    plugin(tree as any, { path: "/src/content/build-orders/tr/18pop-scouts.md" } as any);

    expect(tree.children).toHaveLength(1);
    expect((tree.children[0] as ParagraphNode).children[0].value).toBe("Asıl strateji.");
  });

  it("does not modify files outside the build-orders directory", () => {
    const tree = makeTree([
      { type: "heading", depth: 2, children: [{ type: "text", value: "Dark Age" }] },
      {
        type: "heading",
        depth: 2,
        children: [{ type: "text", value: "Strategy and Follow-Up" }],
      },
      { type: "paragraph", children: [{ type: "text", value: "Content." }] },
    ]);
    const before = tree.children.length;

    plugin(tree as any, { path: "/src/content/units/en/archer.md" } as any);

    expect(tree.children).toHaveLength(before);
  });

  it("does not strip anything when no strategy heading exists", () => {
    const tree = makeTree([
      { type: "heading", depth: 2, children: [{ type: "text", value: "Dark Age" }] },
      { type: "paragraph", children: [{ type: "text", value: "Some steps." }] },
    ]);
    const before = tree.children.length;

    plugin(tree as any, { path: "/src/content/build-orders/en/18pop-scouts.md" } as any);

    expect(tree.children).toHaveLength(before);
  });
});
