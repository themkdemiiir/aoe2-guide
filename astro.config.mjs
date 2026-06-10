// astro.config.mjs

import mdx from "@astrojs/mdx";
import sitemap from "@astrojs/sitemap";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "astro/config";
import icon from "astro-icon";
import pagefind from "astro-pagefind";
import { remarkBuildOrderTruncate } from "./src/lib/remark/build-order-truncate.mjs";

export default defineConfig({
  site: "https://aoe2guide.com",
  output: "static",
  // Prefetch internal links on hover → near-instant navigation (pairs with the
  // ClientRouter view transitions). Hover keeps it conservative on link-dense
  // pages (civs/builds indexes) rather than prefetching everything upfront.
  prefetch: {
    prefetchAll: true,
    defaultStrategy: "hover",
  },
  integrations: [
    mdx(),
    sitemap(),
    pagefind(),
    icon({ include: { lucide: ["*"], heroicons: ["*"] } }),
  ],
  markdown: {
    remarkPlugins: [remarkBuildOrderTruncate],
  },
  vite: {
    plugins: [tailwindcss()],
    build: {
      // Prevent Vite from inlining small font subsets as data: URIs — they would
      // be blocked by font-src 'self' in the CSP. Serve all fonts as /_astro/ URLs.
      assetsInlineLimit: 0,
    },
  },
});
