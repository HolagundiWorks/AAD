# AAD brand guidelines

**Product:** AAD — 2D Architectural Drafting  
**Voice:** Command-first, local-first, paper-first Chrome 2.0 with a bold orange mark.  
**Primary brand color:** Orange (`#E85D04`)  
**Design system:** **AAD Chrome 2.0** (shell) + **AAD Canvas** (drafting overlays) — one mono component language; do not invent one-off controls.

---

## Brand idea

Keep it simple: **orange mark + plain “AAD” text**. No geometric wordmark / stylized logo lockup in the product UI.

The mark is a soft-apex open **A** (white on orange). The name is system UI text next to it.

| Role | Hex | RGB | Use |
|------|-----|-----|-----|
| Brand orange | `#E85D04` | 232, 93, 4 | Mark fill, **focus indicator only** (nav bar, keyboard selection stroke) |
| Brand orange dark | `#C2410C` | 194, 65, 12 | Pressed / hover on orange mark |
| Brand soft | `#FFF6EE` | 255, 246, 238 | Whisper tint — avoid for chips/toggles |
| On-brand | `#FFFFFF` | 255, 255, 255 | Glyph on solid orange mark |
| Text | `#242424` | 36, 36, 36 | Primary UI copy |
| Text secondary | `#6E6E6E` | 110, 110, 110 | Hints, labels, CLI history |
| Layer base | `#F8F8F8` | 248, 248, 248 | Recessed chrome |
| Layer card | `#FFFFFF` | 255, 255, 255 | Top strip / CLI / elevated panels |
| Canvas | `#FFFFFF` | 255, 255, 255 | Drawing paper |
| Stroke / border | `#E8E8E8` / `#D6D6D6` | — | Quiet 1px edges |

Do **not** use Windows blue (`#005FB8`) or Autodesk blue for **brand / chrome**. Keep Fluent System Icons tinted with text; orange only on the brand mark and selected-rail accent bar.

**Canvas selection** may use CAD-familiar blue (`canvas::` tokens in `gui_theme.rs`) so drafting muscle memory stays intact — that blue is not a brand color.

---

## Typography & shapes (AAD Chrome 2.0)

- UI: Segoe UI on Windows (Caption 12 / Body 14 / Title 16 / Subtitle 20); Source Sans 3 for drawing text.
- Titles / section labels: **Semibold** face — never egui `.strong()` (recolor only).
- Product name in chrome: plain **AAD** (semibold), not a custom lettering logo.
- **Radii: all `0` (sharp)** — CAD-dense chrome; controls, cards, dialogs, and docks share one shape.
- **Layout:** one top strip · auto-hide side rails · **command line as hero** (history + input).
- Toggle chips / units: quiet neutrals — **not** orange fills.
- Elevation: 1px strokes on bars/docks; soft ambient shadow on floating windows / modals.
- Control height: **28px**; top bars **32px**; status **28 / 48px**; icons **16px** glyph in **28px** hit (compact rows 14/20).

---

## Voice & naming

- Short name: **AAD**
- Full: **AAD — 2D Architectural Drafting** (AAD is the product name, not an acronym)
- Window title: `AAD`
- Tagline (optional): *Draft the way you always have — fast, predictable, out of your way.*

---

## Assets (source of truth)

| File | Use |
|------|-----|
| `crates/aadt-app/assets/branding/aadt_mark.svg` | App mark (chrome + docs) |
| `crates/aadt-app/assets/branding/aadt_app_icon.svg` | App glyph |
| `crates/aadt-app/assets/branding/app_icon_256.png` | Window / taskbar |
| `crates/aadt-app/assets/branding/favicon.ico` | Favicon |

Logo construction rules: see [logo-guidelines.md](./logo-guidelines.md).

---

## UI application

- Chrome: **mark (20–24px) + “AAD” text** only (right of the top bar).
- Orange for chrome hierarchy only: selection, active nav, units chip, prompts, primary CTAs.
- Do not flood panels with orange fills.
- Compose from mono primitives in `gui_theme.rs` (`button`, `chip_*`, `dialog_frame`, `section_label*`, …) — no raw one-off chrome controls.
- Fluent toolbar icons remain monochrome; tint to text or brand orange when selected.
