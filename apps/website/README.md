# Warsha Marketing Website

Static, zero-dependency, bilingual (Arabic-first RTL + English) marketing site for Warsha (ورشة), built on
**Warsha Design System v3 — «حِرفة» (Hirfa)**.

Live: <https://warsha-dev.pages.dev>

## Design system

`tokens.css` carries the v3 semantic tokens verbatim from `docs/designs/DESIGN_SYSTEM_V3.md` §2.1–2.4
(run `2026-11-09-ui-v3`). Nothing aliases a v2 name.

| Token | Light | Dark | Role |
| --- | --- | --- | --- |
| `--w-bg` | `#F6F4EF` | `#131210` | Warm paper / warm night page background |
| `--w-surface` | `#FFFFFF` | `#1B1A17` | Cards, panels, inputs |
| `--w-sunken` | `#EFECE5` | `#24221E` | Wells, code, table headers |
| `--w-line` / `--w-line-strong` | `#E2DDD3` / `#8C877C` | `#33302A` / `#7D776C` | Decorative dividers / control outlines |
| `--w-text` / `--w-text-muted` / `--w-text-faint` | `#1B1A17` / `#5F5B53` / `#7A756B` | `#F1EEE7` / `#B3AEA3` / `#948F84` | Body / secondary / meta |
| `--w-primary` | `#FFD400` | `#FFD400` | The logo yellow — fill of the one main action |
| `--w-on-primary` | `#1B1A17` | `#1B1A17` | Ink on yellow (12.2:1) |
| `--w-primary-soft` / `--w-primary-text` | `#FFF4C2` / `#7A5C00` | `#3A3008` / `#FFD84D` | Selected states, links, current step |
| `--w-attention` / `--w-attention-soft` | `#A8430A` / `#FFEEDF` | `#FF9D5C` / `#3D2314` | **Waiting on the owner** — nothing else |
| `--w-success` / `--w-danger` / `--w-info` | `#1B6B39` / `#A8261D` / `#1F55A0` | `#5CC98A` / `#F07A70` / `#7DB0F5` | Done / failed / running |
| Radius | `6 / 10 / 14 / 20 / full` | same | Inputs / cards / sheets / composer / pills |
| Motion | `120 / 200 / 320 ms`, `cubic-bezier(.2,.8,.2,1)` | same | Collapses to 0 under `prefers-reduced-motion` |

Rules carried over from the spec:

- **Yellow is spent once per view.** It fills the main action and the logo. Selected rows, chips and tabs use
  `primary-soft` + `primary-text`, never a yellow fill.
- **Orange means one thing only:** a decision is waiting for the owner. Running work is `info` blue.
- **Never colour alone:** every status carries an icon and a word.
- On the light theme, yellow lines and text use `primary-text` (dark gold) because `#FFD400` on paper is ≈1.3:1.

## Files

| File | Purpose |
| --- | --- |
| `index.html` | The page: hero composer, factory, gate, modes, team, numbers, roadmap, FAQ, CTA |
| `tokens.css` | v3 semantic tokens, both themes, density and reduced-motion overrides |
| `style.css` | Layout and components; logical properties only |
| `site.js` | Language, theme, scroll-spy, the factory simulation, gate demo, modes and tabs |
| `assets/fonts.css` + `assets/fonts/` | Vendored IBM Plex Sans Arabic + IBM Plex Mono `woff2` (OFL-1.1) |
| `assets/warsha-mark.svg`, `assets/favicon.svg` | The v3 hexagon mark |
| `404.html` | Not-found page on the same tokens |

## Conventions

- Vanilla HTML/CSS/JS. No framework, no build step, no npm dependencies, no network calls at runtime.
- Logical CSS properties only — no `left`/`right`/`margin-left`/`text-align: left`. Directional icons mirror
  through `[dir='rtl'] .mirror`.
- Bilingual markup uses paired `data-ar` / `data-en` spans switched by `html[lang]`. JS-rendered copy comes from
  `{ ar, en }` pairs and re-renders on toggle. IDs, model names, money and code are wrapped in `<bdi class="ltr">`.
- Themes are `light`, `dark` and `system` (default), persisted in `localStorage` and readable from `?theme=`.
  Language is readable from `?lang=`.
- Fonts are vendored, not fetched from a CDN, so the page renders identically offline.

## The factory simulation

`#factory` runs the five owner-facing stages (الفكرة → الخطة → البناء → الفحص → التسليم) over the real
14-stage / 4-gate pipeline. Its control desk is permanently visible and every control applies live:
play/pause, step, reset, speed, request scenario, budget cap, signal glow, parallel coders, auto-approve,
machine log, track grid, density, randomize and JSON export. Sound is synthesised in the browser, tied to
visible events, and muted from one button.

With auto-approve off the run stops at each gate and waits — the same contract the product enforces.
Exceeding the budget cap halts the run until the cap is raised.

## Verification

```bash
node bridge-io/policy/2026-11-09-website-v3/gates.mjs      # 31 static gates
node bridge-io/website-redesign/serve.mjs apps/website 8791
node bridge-io/website-redesign/verify-v3.mjs             # 92 browser checks + screenshots
```

The static gates assert token fidelity against the approved v3 spec, 24 WCAG contrast pairs, logical-property
purity, ar/en parity, asset and icon resolution, the one-primary rule and the dependency-free boundary.
The browser suite drives host Chrome at 1440×900 and 390×844 in both languages and themes, completes a full
run through all four gates, and checks overflow, console errors, control reachability and reduced motion.

## Deployment

Deployment requires explicit owner approval.

```bash
npx wrangler pages deploy apps/website --project-name warsha-dev --branch main
```