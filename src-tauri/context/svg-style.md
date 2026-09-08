# SVG graphics style — binding for every <svg> you draw

Amber's interface is a quiet, near-monochrome system in the Vercel tradition: zinc neutrals, hairline rules, hierarchy carried by tone and weight rather than hue, and at most one deliberate accent. A chart inside a note must read as part of that surface — not as an image pasted in from another product. Apply these rules whenever you emit an <svg>, and apply them identically to every chart in the same note so they read as a set.

## 1. Canvas and structure
- One <svg> per ```svg fence, with `viewBox="0 0 W H"` and `width="100%"`; no fixed height. Use W between 640 and 720. Height follows the chart type: horizontal bars = 52 per row + 64 for the axis and labels; line charts about 2.4:1; small multiples 200–240 tall.
- `role="img"` plus a `<title>` (referenced by `aria-labelledby`) that states the chart's message, not merely its subject.
- Inner margins in viewBox units: left = longest category label + 14; right 44 for end labels; top 20; bottom 44 for tick labels and one axis title. Nothing touches the edge.
- No outer frame, no background rectangle, no drop shadow, no gradient, no 3-D, no clip art, no emoji. The app already renders the SVG on a bordered surface; a second frame doubles it.
- Work on an 8-unit grid: every x, y, width and height is a multiple of 8 (text baselines may use 4). Derive H from the rows you actually draw — never leave a region taller than one row empty, and never let content run past the bottom margin.

## 2. Ink: color as tone, not hue
- Everything structural is `currentColor` — the app injects the theme's ink (near-black on light, off-white on dark). Never hard-code near-white or near-black (`#fff`, `#000`, `#fcfcfb`, `#18181b`…) for text, axes, marker rings or backgrounds: it breaks one of the two themes.
- Build hierarchy with a fixed opacity ramp on currentColor and nothing else:
  - `1.0` primary marks and direct data labels
  - `0.62` secondary text: category labels, legend text
  - `0.40` tertiary text: tick labels, axis titles, units
  - `0.28` the axis baseline
  - `0.12` grid hairlines
- Chroma budget: at most two hues per note, assigned once and reused in every chart of that note:
  - emphasis `#3b82f6` — the series the note argues for (after, new, improved, target)
  - contrast `#ea580c` — the baseline it is compared against (before, old, control)
  Everything else stays on the ink ramp. A single-series chart uses currentColor at `0.9`, not the emphasis hue — color exists for comparison, not decoration. Never use red or green as data colors: in Amber they are reserved for failure and success states.

## 3. Line and shape grammar
- Hairlines are 1 unit. Grid lines at `0.12` opacity run only in the value direction, never both. The axis baseline is 1 unit at `0.28`. No tick marks — the tick label alone marks the position.
- Bars: 20–24 units tall, gaps at least half the bar height, corner radius 3 (the app's radius scale is 4–6 on larger surfaces; 3 keeps small shapes from reading as pills). Bars start at the baseline; never truncate the value axis.
- Lines: `stroke-width="2"`, `stroke-linejoin="round"`, `stroke-linecap="round"`; no area fill unless the quantity is cumulative. Point markers only on values you annotate: `r="3.5"`, filled with the series color, no ring.
- Multiple series: differentiate by hue first (the two-hue budget), then by `stroke-dasharray="4 4"` when a third series is unavoidable. Never encode a series as a lighter tint of another series' hue — tints read as one series at two confidences.

## 4. Type
- Do not set `font-family`. The SVG inherits the app's type stack and must not diverge from the note body.
- Scale in viewBox units: tick labels 11; category and legend labels 12; direct data labels 12 at `font-weight="600"`; annotations 11.5. Nothing larger — the chart's title is the surrounding markdown heading, not text inside the SVG.
- Numbers: add `style="font-variant-numeric: tabular-nums"` to every numeric text group, use thousands separators, and state the unit once in the axis title (`ms`, `%`, `초`), never on each tick.
- Text uses currentColor at the ramp opacities above. Small uppercase eyebrows, if any, get `letter-spacing="0.04em"`.
- Estimate text width before placing anything: about 0.55 × font-size per Latin letter or digit, 1.0 × font-size per Hangul or CJK glyph, plus 24 units of horizontal padding inside a box. Size boxes from the estimate; if a label will not fit its span, wrap it onto a second line with `<tspan x="…" dy="1.3em">` instead of shrinking below 11 or letting it overhang.

## 5. Composition
- Direct labeling over legends: put values at bar ends and name lines at their right end. A hue key is required whenever two hues are in play (before/after), because a value label cannot say which series it belongs to: one row, bottom-left, of 16-unit swatch lines with 12-unit labels in the `0.62` tone. Single-hue charts need no legend.
- Highlight exactly one thing per chart — the peak, the crossover, the target — with a single 600-weight annotation that says what the reader should notice, not just the number.
- Sort categories by value unless the order is semantic (time, steps). Keep margins, type scale and hue assignment identical across all charts in one note.
- Prefer mermaid for anything structural (flows, sequences, ERDs, state machines). Reserve SVG for quantities: bar, line, dot plot, simple time spans. No pie or donut charts. If a structural diagram must be SVG — because it needs a highlight or annotation mermaid cannot express — lay it out with the rules in §7.

## 6. Data and hygiene
- If the numbers are not in the input, do not invent them silently: say in the note that they are placeholders and keep them easy to replace.
- Follow every chart with a compact markdown table of the same values — it is the accessible, copyable form of the data.
- No <script>, event handlers, external images, fonts or links. Nothing outside the fence.

## 7. Structural diagrams in SVG — lanes, rows, and a collision pass
Sequence and flow diagrams fail in SVG for one reason: coordinates chosen by eye. Compute them.
- **Lanes.** With N participants over an inner width W (left margin 32, right margin 32), lane width is `L = W / N` and lane center `x_i = 32 + L × (i + 0.5)`. The header is a pictogram (§8) centered on `x_i` spanning y 8–28 and the participant label — 12 units, `text-anchor="middle"`, `0.85` tone — at y 44; without a pictogram the label sits at y 28. Lifelines are 1-unit currentColor at `0.28`, drawn exactly at `x_i` from the header bottom + 8 to `H − 32`.
- **Rows.** One event per row with a fixed pitch of 40, the first row 40 below the header bottom: with pictograms `y_k = 96 + 40k`, without them `y_k = 80 + 40k`. Messages, notes and the closing takeaway all take rows; nothing sits between rows and no two events share one. `H = y_last + 40`.
- **Messages.** A line from `x_a` to `x_b` on the row's y: solid for requests, `stroke-dasharray="4 4"` for responses, 1 unit, `0.62` tone. The arrowhead is a 7-unit chevron at the target end in the same tone. The label is centered on the midpoint (`(x_a + x_b) / 2`, `y − 8`) with `text-anchor="middle"`, 11.5 units, `0.62`. If the label is wider than `|x_b − x_a| − 16`, wrap it to two tspans or shorten it — it must never overhang a neighboring lane.
- **Notes.** A note over lanes i…j spans `x_i − L/2 + 8` to `x_j + L/2 − 8`, height 28 for one line or 44 for two, radius 4, fill currentColor at `0.04`, stroke `0.28`, text centered at 11.5. A note occupies its own row. A note over a single lane is centered on that lane; if its text needs more than `L − 16`, widen it symmetrically around `x_i` (it may then cross the neighboring lifelines) — never shift it off-center to avoid them.
- **Highlight.** Exactly one box carries the insight: stroke `#ea580c` at 1.5 units, fill `#ea580c` at `0.08`, text stays currentColor at `0.9`. Its takeaway sentence sits alone in the last row, 12 units at weight 600, left-aligned to the first lane center.
- **Collision pass before emitting.** Check every label against its span, every box against the lifelines it is allowed to cross, every arrow end against its lane center, and every row against the 40 pitch. Fix the coordinates; do not leave the gap, the overlap or the empty band.

## 8. Pictograms and shapes — the app's icon language, never emoji
Text-only diagrams read slowly; a small line icon above each participant or beside each entity tells the eye what kind of thing it is before the label is read. Use the same visual language as the app's icons: Lucide-style line pictograms on a 24-unit grid, `stroke="currentColor" stroke-width="1.75" fill="none" stroke-linecap="round" stroke-linejoin="round"`, drawn at 20 units via `<g transform="translate(x − 10, y) scale(0.8333)">`, tone `0.85`. No emoji: color glyphs break the monochrome surface and render differently on every OS.
- **Where.** One pictogram per participant (sequence header), per node (flow/lane diagram) or per category label when the categories are kinds of things (services, stores, people). Charts about quantities of one kind get none. Never place an icon as filler in empty space, never more than one per element, never inside a message label.
- **Vocabulary — pick by role, not by mood.**
  - service / consumer / producer / module → `box`
  - broker / coordinator / server / node → `server`
  - database / store / cache → `database`
  - topic / partitions / log segments / layers → `layers`
  - user / client / person → `user`
  - file / note / config → `file`
  - folder / vault / directory → `folder`
  - timer / timeout / schedule / session → `clock`
  - message / event / request → `message`
  - network / external API / internet → `globe`
  - worker / thread / process / CPU → `cpu`
- **Paths (24-unit grid, use verbatim).**
  - `box`: `<path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z"/><path d="m3.3 7 8.7 5 8.7-5"/><path d="M12 22V12"/>`
  - `server`: `<rect width="20" height="8" x="2" y="2" rx="2"/><rect width="20" height="8" x="2" y="14" rx="2"/><path d="M6 6h.01"/><path d="M6 18h.01"/>`
  - `database`: `<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14a9 3 0 0 0 18 0V5"/><path d="M3 12a9 3 0 0 0 18 0"/>`
  - `layers`: `<path d="m12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z"/><path d="m22 12.5-8.58 3.91a2 2 0 0 1-1.66 0L2.6 12.5"/><path d="m22 17.65-8.58 3.91a2 2 0 0 1-1.66 0L2.6 17.65"/>`
  - `user`: `<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>`
  - `file`: `<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/>`
  - `folder`: `<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>`
  - `clock`: `<circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/>`
  - `message`: `<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>`
  - `globe`: `<circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/>`
  - `cpu`: `<rect x="4" y="4" width="16" height="16" rx="2"/><rect x="9" y="9" width="6" height="6"/><path d="M15 2v2"/><path d="M15 20v2"/><path d="M2 15h2"/><path d="M2 9h2"/><path d="M20 15h2"/><path d="M20 9h2"/><path d="M9 2v2"/><path d="M9 20v2"/>`
- **Shapes carry roles too.** Component or actor = rounded rectangle (radius 4, stroke `0.28`, fill currentColor `0.04`); store = the `database` pictogram, not a drawn cylinder; queue or topic = a pill (radius = half height); state or step = rounded rectangle with the step number in a 600-weight label. No diamonds, clouds or 3-D blocks — those belong to mermaid. Shapes never overlap; connect them with the same 1-unit lines, chevron arrowheads and centered labels as §7.
