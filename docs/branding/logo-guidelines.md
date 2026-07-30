# AAD logo design guidelines

## Concept

**Simple mark + plain text.**  
Orange square with a soft-apex open **A**, paired with the word **AAD** in system UI type. No stylized geometric wordmark in product chrome.

## Mark anatomy

1. **Field** — solid `#E85D04`, square corners.
2. **Glyph** — white open A; monoline; round caps; soft apex; short crossbar.
3. **Name** — “AAD” as plain text beside the mark (not drawn into the SVG).

## Variants

| Variant | When |
|---------|------|
| **Mark** (`aadt_mark.svg`) | In-app chrome, README, docs |
| **App icon** (`aadt_app_icon.svg` / PNGs) | Taskbar, alt-tab, favicon |

## Clear space & sizing

- Clear space: ≥ ¼ of mark height on all sides.
- Chrome: mark **20–24** px; text beside it at default strong label size.
- Minimum digital mark: **16×16** px.

## Do

- Keep the mark simple and bold.
- Pair with plain “AAD” text.
- Use brand orange or white for the glyph only.

## Don’t

- Don’t put a custom lettering logo next to the mark in chrome.
- Don’t fill the A triangle solid.
- Don’t add glows, grids, or gradients to the master SVG.

## File export

- Master: `aadt_mark.svg`.
- Raster: PNG 16–512 + `favicon.ico`.
- Window icon: `app_icon_256.png` via eframe `with_icon`.
