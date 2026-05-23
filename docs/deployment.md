# Deployment — Cloudflare Pages

The site is a fully static build with no server runtime. Cloudflare Pages serves `dist/` directly from CDN.

## One-time setup

1. Sign in to Cloudflare → **Workers & Pages** → **Create application** → **Pages** → **Connect to Git**.
2. Select the `aoe2-guide` repo.
3. Build settings:
   - **Framework preset:** *Astro* (or leave as None; the explicit command below is what matters)
   - **Build command:** `pnpm install --frozen-lockfile && pnpm build`
   - **Build output directory:** `dist`
   - **Root directory:** (leave blank)
4. Environment variables:
   - `NODE_VERSION=20` (matches `.nvmrc`)
   - `PNPM_VERSION=9` (or whatever pnpm version is in the lockfile)
5. **Production branch:** `main`. Preview deploys are auto-created for every other branch and PR.

## Custom domain

In the Pages project → **Custom domains** → add your domain. If the domain is already on Cloudflare, DNS is wired automatically. Otherwise, add the CNAME record manually.

## Cache headers

Cloudflare's default caching is fine for `dist/`. To extend:

Create `public/_headers` (Cloudflare Pages serves this verbatim):

```
/images/aoe2/*
  Cache-Control: public, max-age=31536000, immutable

/_astro/*
  Cache-Control: public, max-age=31536000, immutable

/fonts/*
  Cache-Control: public, max-age=31536000, immutable
```

## Redirects

`public/_redirects` handles 404 → home or other rewrites:

```
/old-path  /en/  301
```

## Build performance

- Astro build time grows linearly with content count. Typical: 1.5–3s.
- Pagefind indexing adds ~1–3s.
- Cloudflare's pnpm cache speeds repeat builds significantly once warmed.

## Troubleshooting

- **Build fails on `pnpm install`**: ensure `pnpm-lock.yaml` is committed and the Node version matches `.nvmrc`.
- **Build fails on `astro check`**: TS errors. Run `pnpm check` locally.
- **Pagefind index missing in deploy**: confirm the `pagefind` integration is in `astro.config.mjs`. The plugin runs as part of `astro build`, not as a separate step.
- **Fonts 404 on production**: `@fontsource` packages are bundled by Vite; if 404s appear, run `pnpm install` and `pnpm build` to verify the lockfile is intact.

## DNS / domain transfer

Out of scope for this doc. Use Cloudflare's domain transfer flow.
