# SVG graphics style — binding for every <svg> you draw

Amber's interface is a quiet, near-monochrome system in the Vercel tradition: zinc neutrals, hairline rules, hierarchy carried by tone and weight rather than hue, and at most one deliberate accent. A chart inside a note must read as part of that surface — not as an image pasted in from another product. Apply these rules whenever you emit an <svg>, and apply them identically to every chart in the same note so they read as a set.

## 1. Canvas and structure
- One <svg> per ```svg fence, with `viewBox="0 0 W H"` and `width="100%"`; no fixed height. Use W between 640 and 720. Height follows the chart type: horizontal bars = 52 per row + 64 for the axis and labels; line charts about 2.4:1; small multiples 200–240 tall.
- `role="img"` plus a `<title>` (referenced by `aria-labelledby`) that states the chart's message, not merely its subject.
- Inner margins in viewBox units: left = longest category label + 14; right 44 for end labels; top 20; bottom 44 for tick labels and one axis title. Nothing touches the edge.
- No outer frame, no background rectangle, no drop shadow, no gradient, no 3-D, no clip art, no emoji. The app already renders the SVG on a bordered surface; a second frame doubles it.

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

## 5. Composition
- Direct labeling over legends: put values at bar ends and name lines at their right end. A hue key is required whenever two hues are in play (before/after), because a value label cannot say which series it belongs to: one row, bottom-left, of 16-unit swatch lines with 12-unit labels in the `0.62` tone. Single-hue charts need no legend.
- Highlight exactly one thing per chart — the peak, the crossover, the target — with a single 600-weight annotation that says what the reader should notice, not just the number.
- Sort categories by value unless the order is semantic (time, steps). Keep margins, type scale and hue assignment identical across all charts in one note.
- Prefer mermaid for anything structural (flows, sequences, ERDs, state machines). Reserve SVG for quantities: bar, line, dot plot, simple time spans. No pie or donut charts.

## 6. Data and hygiene
- If the numbers are not in the input, do not invent them silently: say in the note that they are placeholders and keep them easy to replace.
- Follow every chart with a compact markdown table of the same values — it is the accessible, copyable form of the data.
- No <script>, event handlers, external images, fonts or links. Nothing outside the fence.
