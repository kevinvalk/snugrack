# Rackraft identity

This package contains real editable SVG geometry. Transparent background. No external images or font files.

## Files

- `rackraft-logo.svg`: adaptive header logo (mark + outlined Inter Display SemiBold wordmark)
- `rackraft-mark.svg`: adaptive standalone mark, also useful as a favicon
- `rackraft-logo-light.svg` and `rackraft-logo-dark.svg`: forced mode variants
- `rackraft-mark-light.svg` and `rackraft-mark-dark.svg`: forced mode variants
- `rackraft-mark-flat.svg` and `rackraft-mark-flat-dark.svg`: always-flat versions
- `favicon-light-*`, `favicon-dark-*`: raster exports, deliberately flat below 32px
- `favicon-light.ico`, `favicon-dark.ico`: multiple sizes
- `rackraft-preview.png`: visual contact sheet

## Geometric consistency

The rail face, its holes, the top sheet, and the lower R leg share an isometric 30-degree axis. Top-edge vectors are (67,39), (21,12), and (92,53); fold/depth is (38,-22). The lower leg follows parallel edges rather than narrowing at the tip. Cutouts are transparent, not white paint.

## Conditional shading and theme

External SVGs loaded with `<img>` get their own SVG viewport. The CSS media query `@media (max-height:31px)` hides the shading layers, leaving the same geometry with flat colors. At 32px or more it displays subtle folding gradients. The default SVG also follows `prefers-color-scheme: dark`.

For a Svelte app:

```svelte
<img src="/rackraft-logo.svg" alt="Rackraft" height="36" />
```

The mode of a remote SVG image is isolated from the page's `.dark`/`.light` class. If your app's theme toggle overrides the OS, choose explicit fixed-theme assets:

```svelte
<img src={dark ? '/rackraft-logo-dark.svg' : '/rackraft-logo-light.svg'} alt="Rackraft" height="36" />
```

For favicon:

```html
<link rel="icon" href="/rackraft-mark.svg" type="image/svg+xml" />
```

**Important:** If you inline the SVG source directly in HTML, its media query measures the **page viewport**, not the icon element's height. For inline use, select the flat asset yourself or control `.rr-shading` with your own CSS. For reliability, raster ICO/PNG assets are also included.

## Typography

Wordmark: Inter Display SemiBold, optically tightened, with glyphs converted to vector outlines; no font dependency. Use Inter/Inter Display in the application UI if desired. The main project name is `Rackraft` (not `Rackkraft`).
