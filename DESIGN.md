# DESIGN.md — Barangay ID System

Single source of truth for the app's visual system. Style every screen from
these tokens; do not introduce off-system colors, sizes, shadows, or fonts.
Lint: `npx @google/design.md lint DESIGN.md`.

## Overview

Civic-operate system for barangay hall officers: paper surfaces, navy primary
actions, gold reserved for focus and emphasis. Kiosk-first — large touch
targets (≥40px), big tabular numerals, always-visible stats, no collapsible
sections. Motion is restrained and always honors `prefers-reduced-motion`.

## Tokens (mirrors `src/css/style.css` `:root`)

```yaml
colors:
  navy: "#0F2B5E"        # primary actions, nav, male data
  navy-hover: "#0a1f45"
  gold: "#C79C14"        # accent, required marks, female data
  gold-ring: "#F59E0B"   # focus ring only
  ink: "#0F172A"         # body text
  muted: "#64748B"       # secondary text
  paper: "#F8FAFC"       # page background
  surface: "#FFFFFF"     # cards, tables, forms
  border: "#E2E8F0"
  border-strong: "#CBD5E1"
  info: "#2563EB"        # Renew status, input focus
  success: "#059669"     # Active status
  warning: "#D97706"
  error: "#DC2626"       # Lost status, destructive
  damage: "#C2410C"      # Damage status (pill text)
typography:
  sans: "Inter, Segoe UI, system-ui, sans-serif"   # body/data
  display: "Source Serif Pro, Georgia, serif"      # page + section titles
  mono: "JetBrains Mono, Menlo, monospace"         # IDs, numbers
  scale: [11px, 13px, 15px, 18px]                  # --text-xs/sm/base/lg
rounded:
  card: 16px
  input: 12px
  chip: 8px
  pill: 999px
spacing:
  grid: 8px                                                           # gaps 4/8/12/16/20/24
  card-padding: "20-24px"
  section-gap: "16-20px"
elevation:
  card: "0 1px 2px rgba(16,24,40,.05), 0 4px 12px rgba(16,24,40,.06)"  # --shadow-card; only named level
```

## Colors

70/20/10 discipline: 70% neutral (paper/surface/border), 20% navy, 10% gold.
Gold is never a large fill — focus ring and emphasis only. Status color is
functional, never decorative: Active=success, Renew=info, Lost=error,
Damage=#C2410C. All status pills pair a tinted background, dark text, and a
matching border (e.g. `.badge-active`, `.status-pill--*`).

## Typography

Inter (400–700) for everything read; Source Serif Pro 600 only for page and
kiosk section titles (`--font-display`); JetBrains Mono for IDs and numerals.
Stat values are `font-variant-numeric: tabular-nums`. All fonts are bundled
TTFs under `src/fonts/` — never a CDN. Uppercase is applied via `.field-upper`
or explicit `text-transform` on structural labels (11px/700/.1em), not on
values.

## Layout

Sticky full-bleed navy nav (70/20/10 gold rule), `max-w-[1280px]` main.
Dashboard is the kiosk hub: `.kiosk-head` → Registry overview (3
`.kiosk-stat` cards) → Card status (4 tinted `.kiosk-stat` cards) → Services
(3 `.kiosk-tile` buttons). Every spoke view starts with `.view-head`
(back pill + title). Forms use `.form-section` cards; tables use `.card` +
`.data-table`. Breakpoints: 640px (stack grids, single column), 1024px
(3-col forms).

## Elevation

One named level: `--shadow-card`. No invented blur/opacity values. Borders
carry most separation; shadow only on hover-lift and the toast.

## Shapes

Radius roles are fixed: card 16px, input 12px, chip 8px, pill 999px. Icons are
24px stat / 32px tile PNG glyphs (`src/icons/`) inside bordered wells; no
Unicode-scaled glyphs.

## Components & states

Every interactive element inherits the global contract:
`:focus-visible` → 2px gold-ring outline (inputs: info border + 4px ring);
`:active` → `scale(.985)` (nav tabs opt out); `:disabled` → `.55` opacity,
`not-allowed`. Key components: `.action-btn` (+`--primary/secondary/view/edit/
renew/delete/warning`), `.action-chip`/`.settings-chip`, `.hub-back`,
`.kiosk-tile`, `.kiosk-stat`, `.status-pill`, `.badge-*`, `.form-field__input/
select`, `.toast`, `.empty-state`, `.settings-progress__*`, `.detail-field`.
Loading: `.skeleton`; busy submit uses `:disabled` + `.opacity-55`.

## Do's & Don'ts

- Do use the semantic aliases (`--color-primary`, `--color-error`, …) in new CSS.
- Do keep stat numerals ≥28px, tabular, and token-colored per variant.
- Don't add new colors, shadows, radii, or font families.
- Don't add hover-only affordances to non-interactive elements (kiosk stats are read-only regions; hover lift is reserved for `.kiosk-tile`/cards that act).
- Don't add collapsible sections on the dashboard — kiosk rule.
- Don't ship a transition without the global `prefers-reduced-motion` fallback (already global; don't override with `!important` except the documented nav-tab opt-out).
