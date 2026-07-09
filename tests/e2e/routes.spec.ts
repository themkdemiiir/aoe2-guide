import { expect, test } from "@playwright/test";

// Asserts that key canonical routes return 200 and render a meaningful title.
// Add new routes here when new sections ship.

const routes: Array<{ path: string; titleContains: string }> = [
  { path: "/en/", titleContains: "AOE2" },
  { path: "/tr/", titleContains: "AOE2" },
  { path: "/en/civs/", titleContains: "Civilization" },
  { path: "/tr/civs/", titleContains: "Uygarlık" },
  { path: "/en/civs/britons/", titleContains: "Britons" },
  { path: "/en/units/", titleContains: "Unit Guide" },
  { path: "/en/units/longbowman/", titleContains: "Longbowman" },
  { path: "/en/builds/", titleContains: "Build Orders" },
  { path: "/en/maps/", titleContains: "Map Guides" },
  { path: "/en/matchups/", titleContains: "Comparator" },
  { path: "/en/analyzer/", titleContains: "Analyzer" },
  { path: "/tr/analyzer/", titleContains: "Analiz" },
  { path: "/en/learn/", titleContains: "Learn" },
  { path: "/en/glossary/", titleContains: "Glossary" },
  { path: "/en/search/", titleContains: "Search" },
];

for (const { path, titleContains } of routes) {
  test(`route ${path} returns 200 and title contains "${titleContains}"`, async ({
    page,
    request,
  }) => {
    const response = await request.get(path);
    expect(response.status()).toBe(200);
    await page.goto(path);
    await expect(page).toHaveTitle(new RegExp(titleContains, "i"));
  });
}

test("removed locale /es/ returns 404", async ({ request }) => {
  const response = await request.get("/es/");
  expect(response.status()).toBe(404);
});

test("removed locale /de/ returns 404", async ({ request }) => {
  const response = await request.get("/de/");
  expect(response.status()).toBe(404);
});

// Civ TR text has been fully sourced from aoe2techtree since 2026-06 — the
// "not translated yet" fallback banner only exists on MD content (learn/blog),
// and no MD entry is currently missing its TR file. Assert the TR page serves
// real Turkish content instead.
test("TR civ page serves localized content at the TR URL", async ({ page, request }) => {
  const response = await request.get("/tr/civs/armenians/");
  expect(response.status()).toBe(200);

  await page.goto("/tr/civs/armenians/");
  await expect(page).toHaveTitle(/Armenians/i);
  await expect(page.getByRole("heading", { name: "Uygarlık Bonusları" })).toBeVisible();
});

test("comparator restores selection from URL params", async ({ page }) => {
  await page.goto("/en/matchups/?a=mongols&b=mayans");
  const selA = page.locator("#civ-a");
  const selB = page.locator("#civ-b");
  await expect(selA).toHaveValue("mongols");
  await expect(selB).toHaveValue("mayans");
});
