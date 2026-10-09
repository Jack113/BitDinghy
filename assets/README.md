# BitDinghy website + desktop assets

A separated asset pack based on the BitDinghy brand-board artwork.

## Quick install

1. Copy the **contents** of this folder into your website's public/static `assets/` folder.
2. Paste `web/head-snippet.html` into the site's `<head>` (adjust absolute paths if your site serves assets elsewhere).
3. Choose `logos/horizontal-logo-dark.svg` for dark headers or `horizontal-logo-light.svg` for light headers.
4. For Windows desktop app packaging, use `icons/windows-app.ico` as the Windows application icon. Keep the 256 and 512 PNGs for stores and installers.
5. Theme CSS is in `themes/themes.css`; toggle `<html data-theme="dark">` or `data-theme="light"`.

## Folder structure

- `logos/` - transparent SVG and PNG primary / horizontal wordmarks and digital boat illustration.
- `icons/` - favicon SVG/ICO/PNGs, scalable icon, app icon, Windows ICO, mobile/PWA icons.
- `brand/` - original 8-color palette (JSON/CSS), typography guidance.
- `themes/` - functional light/dark CSS variables and optional initial theme script.
- `previews/` - separately cropped **exact** sections of the original brand board (reference/concepts only).
- `web/` - web manifest, HTML head snippet, social preview OG image.
- `source/` - original unchanged brand board and the original binary sailboat concept.

## Important notes

- The original artwork was raster AI-generated. `previews/` and `source/` preserve its look exactly.
- `logos/*.svg` and `icons/*.svg` are **clean vector adaptations**, not traced, pixel-perfect copies of the source artwork. These are transparent and easier to scale.
- Theme screenshots are visual mockups, not finished screens or complete implementations.
- The green shown as Primary Green (`#57F000`) comes from the supplied reference. For light-theme UI button/text contrast, the CSS uses Deep Green (`#1F8F46`) instead.
- Verify asset paths, browser icons, and app packaging on your actual deployment. No image file contains installed fonts.
