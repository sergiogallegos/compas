# compas — website

Static marketing/landing page for compas. No build step — plain HTML/CSS/JS.

## Local preview
Open `index.html` in a browser, or serve the folder:

```bash
npx serve website        # or: python -m http.server -d website 8080
```

## Deploy
Any static host works. Point it at this `website/` directory:

- **GitHub Pages** — set Pages source to `/website` (or a `gh-pages` deploy of this folder).
- **Netlify / Vercel / Cloudflare Pages** — publish directory: `website`, no build command.

## Download buttons
The two primary buttons link to `https://github.com/sergiogallegos/compas/releases/latest`.
Once the release workflow publishes Windows (`.msi`/`.exe`) and macOS (`.dmg`) installers as
release assets, those links resolve to the latest build. A small script highlights the installer
matching the visitor's OS.

## Assets
`assets/logo.png` and `assets/screenshot.png` are copied from the app's `docs/assets/`. Regenerate
the screenshot from the running app and re-copy when the UI changes.
