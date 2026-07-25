# AADT brand guidelines

**Product:** AADT — AI-Aided Architecture Design and Takeoff  
**Voice:** Command-first, local-first, calm Fluent chrome with a bold orange identity.  
**Primary brand color:** Orange (`#E85D04`)

---

## Brand idea

Keep it simple: **orange mark + plain “AADT” text**. No geometric wordmark / stylized logo lockup in the product UI.

The mark is a soft-apex open **A** (white on orange). The name is system UI text next to it.

| Role | Hex | RGB | Use |
|------|-----|-----|-----|
| Brand orange | `#E85D04` | 232, 93, 4 | Mark fill, selection, focus, CTAs |
| Brand orange dark | `#C2410C` | 194, 65, 12 | Pressed / hover on orange |
| Brand soft | `#FFF0E6` | 255, 240, 230 | Soft accent fills |
| On-brand | `#FFFFFF` | 255, 255, 255 | Glyph on solid orange mark |
| Text | `#242424` | 36, 36, 36 | Primary UI copy (Fluent grey) |
| Text secondary | `#616161` | 97, 97, 97 | Hints, labels |
| Layer base | `#F3F3F3` | 243, 243, 243 | Recessed chrome |
| Layer card | `#FFFFFF` | 255, 255, 255 | Elevated panels / cards |
| Canvas | `#FFFFFF` | 255, 255, 255 | Drawing paper |
| Stroke / border | `#E5E5E5` / `#C8C8C8` | — | 1px Fluent edges |

Do **not** use Windows blue (`#005FB8`) for brand. Keep Fluent System Icons tinted with text/orange only.

---

## Typography & shapes (Fluent 2)

- UI: Segoe UI on Windows (Caption 12 / Body 14 / Subtitle 20); Source Sans 3 for drawing text.
- Product name in chrome: plain **AADT** (strong), not a custom lettering logo.
- Controls: 4px radius · cards/menus: 8px · dialogs: 12px · edge-docked bars: 0.
- Elevation: 1px strokes + soft ambient shadow on floating windows.

---

## Voice & naming

- Short name: **AADT**
- Full: **AI-Aided Architecture Design and Takeoff**
- Window title: `AADT`
- Tagline (optional): *Draft the way you always have. Let AADT handle the takeoff.*

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

- Chrome: **mark + “AADT” text** only (right of the top bar).
- Orange for hierarchy only: selection, active nav, units chip, prompts.
- Do not flood panels with orange fills.
- Fluent toolbar icons remain monochrome; tint to text or brand orange when selected.
