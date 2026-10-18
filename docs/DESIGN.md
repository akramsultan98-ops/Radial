# Design system

Defined in `src/styles/tokens.css`; components use tokens only.

## Principles

- **The ring is the product.** The workspace gives the radial selector the centre stage; the side
  panel carries details and options, and shows only what applies to the chosen conversion.
- **Quiet surfaces, one accent.** Near-black, slightly cool neutrals; a single periwinkle accent
  marks selection, focus and the primary action. Status colours appear only for status.
- **Every action answers.** Hover, armed (drag), selected, busy, success and error each have a
  distinct visual and an accessible announcement (status bar `aria-live`).
- **Motion explains.** Segments unfold on entry, push outward on hover, the orb follows a drag,
  and an orbit traces conversion stages. All motion collapses under reduced motion.

## Tokens

| Group | Tokens |
| --- | --- |
| Surfaces | `--surface-0` app · `--surface-1` panels · `--surface-2` raised · `--surface-3` hover · `--surface-4` pressed |
| Lines | `--line-1/2/3` (6 % / 10 % / 16 % white) |
| Text | `--text-1` 16.4:1 · `--text-2` 8.3:1 · `--text-3` 5.0:1 (minimum for body text) |
| Accent | `--accent`, `--accent-strong`, `--accent-soft`, `--accent-line`, `--accent-glow` |
| Status | `--success`, `--warning`, `--danger` (+ `-soft` backgrounds) |
| Type | Inter Variable (bundled), sizes 11 / 12.5 / 13.5 / 15 / 19 / 26, weights 430 / 520 / 600 |
| Space | 4 px grid: `--space-1` … `--space-14` |
| Radius | 6 / 10 / 14 / full |
| Elevation | `--shadow-1` (controls) · `--shadow-2` (popovers) · `--shadow-3` (orb, dialogs) |
| Focus | `--focus-ring`: 2 px gap + 2 px accent |
| Motion | `--dur-1` 110 ms · `--dur-2` 200 ms · `--dur-3` 340 ms · `--ease-out` · `--ease-spring` · `--stagger` 28 ms |

## Reduced motion

`prefers-reduced-motion: reduce` sets every duration to 0 unless the user chose **Full** in
Settings; choosing **Reduced** forces it regardless of the OS. The conversion spinner keeps a slow
rotation so progress remains perceivable.

## The radial selector

- Segments are annular sectors laid out clockwise from 12 o'clock with a 4° gap
  (`radial/geometry.ts`). Inner radius is 62 % of outer; labels sit on the mid-radius.
- States: default · hover (fill lifts, segment pushes 7 px outward) · keyboard focus (accent
  outline) · selected (accent gradient) · armed during drag (accent stroke + glow) · unavailable
  (hatched, dimmed, explains why) · busy (others fade to 35 %).
- Size follows the stage (300–620 px) and never clips: the ring is computed from the stage's
  content box minus margins, and the drag offset is clamped to the label radius.
- The orb holds a real preview of the file (from the native pipeline), its name, and either its
  format and size or the chosen route (PNG → JPEG).

## Accessibility checklist

- Ring: `radiogroup` of `radio`s with roving tab stop; ← → ↑ ↓, Home/End, Enter/Space, 1–9.
- An equivalent list of buttons in the side panel for users who prefer it.
- Dialogs: focus moves in, Tab is trapped, Escape closes, focus returns to the trigger.
- Visible focus on every control; tooltips on icon-only buttons; status announcements via
  `aria-live`.
- Keyboard shortcuts: `Ctrl+O` open · `Ctrl+Enter` convert · `Esc` cancel/back · `Ctrl+H`
  history · `Ctrl+,` settings.
