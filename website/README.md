# compas — website

Static site for compas. No build step — plain HTML/CSS/JS.

Pages:
- `index.html` — landing page (hero, screenshot, features, download).
- `manual.html` — full user manual (every function).
- `privacy.html` — privacy policy.

## Local preview
Open `index.html` in a browser, or serve the folder:

```bash
npx serve website        # or: python -m http.server -d website 8080
```

## Deploy — Cloudflare Pages (live host)

`compasaudio.com` is served by **Cloudflare Pages** from this repo. To (re)connect:

1. Cloudflare dashboard → **Workers & Pages** → **Create** → **Pages** → **Connect to Git**;
   pick the `sergiogallegos/compas` repo.
2. Build settings: **Framework preset = None**, **Build command = (empty)**,
   **Build output directory = `website`**. Save & deploy.
3. **Custom domains** → add `compasaudio.com` (and `www`). Since the domain's DNS is already on
   Cloudflare, the records are created automatically; HTTPS is provisioned for you.

Every push to `main` redeploys automatically. The `CNAME` file in this folder is a GitHub Pages
artifact and is ignored by Cloudflare Pages — harmless to leave, since it documents the domain.

> Alternative: any static host works (publish directory `website`, no build command). For GitHub
> Pages, set the Pages source to this folder and point a Cloudflare DNS record at GitHub Pages.

## Download buttons
The two primary buttons link to `https://github.com/sergiogallegos/compas/releases/latest`.
Once the release workflow publishes Windows (`.msi`/`.exe`) and macOS (`.dmg`) installers as
release assets, those links resolve to the latest build. A small script highlights the installer
matching the visitor's OS.

## Assets
`assets/logo.png` and `assets/screenshot.png` are copied from the app's `docs/assets/`. Regenerate
the screenshot from the running app and re-copy when the UI changes.
