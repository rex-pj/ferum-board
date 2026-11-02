# qhortus.com brand mark

Site branding for the qhortus.com deployment. It is applied through `/admin/settings`,
not through the repository's default theme, so other Ferum Board installs are unaffected.

## Applying it

On `/admin/settings`, under branding:

| Field   | Upload          | Why this file                                                                 |
| ------- | --------------- | ----------------------------------------------------------------------------- |
| Logo    | `logo-512.png`  | The mark on its own ink tile, so one image reads on both the light and dark site theme |
| Favicon | `favicon.ico`   | 16, 32 and 48 px in one file                                                   |

Set **Site name** to `qhortus`. The nav renders the name beside the logo, so the
uploaded image is the mark alone, never a lockup.

The SVGs cannot be uploaded: the logo and favicon endpoints accept raster formats
only, because an uploaded SVG served from `/files/` is a stored-XSS vector.

## Files

| File                         | Use                                                    |
| ---------------------------- | ------------------------------------------------------ |
| `qhortus-mark.svg`           | Master mark for light backgrounds (print, decks)       |
| `qhortus-mark-reversed.svg`  | Master mark for dark backgrounds                       |
| `qhortus-tile.svg`           | Source of the two uploads                              |
| `build.mjs`                  | Generates everything above; edit it, never the outputs |

## Geometry

Everything derives from the disc radius `R` and φ ≈ 1.618:

- Doorway: width `R/φ`, height `R` (a golden rectangle), arched top, centred in the disc.
- Tail: thickness `R/φ³`, at 45° (the set-square angle), overshooting the disc by `R/φ²`.
- Gap between tail and disc: `R/φ³ ÷ φ²`, cut by a mask so it stays transparent.

Colours: ink `#17322A`, brass `#B0843A`, stone `#DCE5D8`, brass on dark `#D2A658`.
