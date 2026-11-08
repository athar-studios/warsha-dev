# Warsha Marketing Website

Static, zero-dependency, bilingual (Arabic-first RTL + English toggle) marketing website for Warsha (ورشة), adapted to the NVIDIA industrial design system.

## Design Tokens

| Token | Value | Role |
| --- | --- | --- |
| `--bg-dark` | `#000000` | True black page background for dark sections |
| `--bg-light` | `#ffffff` | Pure white background for light sections |
| `--plate-dark` | `#1a1a1a` | Dark surface / card background |
| `--rule` | `#5e5e5e` | Borders, ledger dividers, section borders |
| `--signal` | `#FFD60A` | Signature yellow accent on dark surfaces (borders, underlines, signals) |
| `--signal-light` | `#8a6a00` | Amber-yellow for text & essential borders on white (WCAG AA 5.12:1) |
| `--signal-bright` | `#FFE866` | Bright yellow hover highlight on dark |
| `--text-dark` | `#ffffff` | Primary text on dark backgrounds |
| `--text-light` | `#000000` | Primary text on light backgrounds |
| `--muted-on-dark` | `#a7a7a7` | Muted secondary text on dark surfaces |
| `--muted-on-light` | `#757575` | Muted secondary text on light surfaces |
| `--meta` | `#898989` | Secondary metadata and labels |
| `--link-hover` | `#3860be` | Blue link hover state |
| `--btn-hover` | `#1eaedb` | Teal button hover state |
| `--btn-active` | `#007fff` | Active/pressed button state |
| `--shadow` | `0 0 5px rgba(0,0,0,.3)` | Ambient elevation shadow |
| Radius | `2px` | Minimal engineered radius throughout |

## Architecture & Conventions

- Vanilla HTML5, CSS3, JavaScript (no framework, no build step, no npm dependencies).
- Pure logical CSS properties (`margin-inline`, `padding-inline`, `border-inline`, `inset-inline`, `text-align: start/end`).
- Bilingual markup paired with `data-ar` and `data-en` spans toggled via `html[lang]`.
- System Latin fonts (`Arial, Helvetica, sans-serif`) with Google Fonts for `IBM Plex Sans Arabic` and `JetBrains Mono`.

## Deployment

Deploying requires explicit owner approval (Gate C / Owner gate).

```bash
npx wrangler pages deploy apps/website --project-name warsha-dev
```
