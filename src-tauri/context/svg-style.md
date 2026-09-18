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
- **Never fill anything with the page's background color — you do not know what it is.** An SVG written here also ends up pasted into a blog, a README or a slide, where the app's tokens do not exist. A rect filled with a guessed background (`#fff`, `var(--surface, #ffffff)`, `Canvas`) turns into a white slab on a dark page and swallows the `currentColor` text on top of it. Boxes are **translucent tints only** — `currentColor` or a budget hue at low opacity — so whatever is behind them shows through and the text keeps its contrast in both themes.
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
- **Boxes that carry text are tints, and the lines behind them are broken, not covered.** A sequence-diagram note, a callout, a legend chip or a label on a lifeline is drawn as (1) a `<rect rx="4">` sized from the text estimate plus 12 units of padding each side, filled `currentColor` at `0.04` (or the contrast hue at `0.08` for the one highlight), (2) its stroke, (3) the text. **Nothing opaque is ever painted underneath.** So a line must not run through the box in the first place: draw that line in segments that stop 8 units before the box and resume 8 units after it. For a lifeline at `x` crossed by a box spanning `y1…y2`, emit `M x,top V y1−8` and `M x,y2+8 V bottom` instead of one full-height path. This is also how a reader expects a sequence diagram to look — the note interrupts the lifeline.
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
- SVG has three subjects: **quantities** (bar, line, dot plot, time spans), **structures** that need a highlight or annotation mermaid cannot express (§7), and **geometry** where the shape, the tiling or the size ratio is itself the content (§9). Which format a figure gets is decided by the note prompt, not here — this guide only governs how the SVG looks. No pie or donut charts.

## 6. Data and hygiene
- If the numbers are not in the input, do not invent them silently: say in the note that they are placeholders and keep them easy to replace.
- Follow every chart with a compact markdown table of the same values — it is the accessible, copyable form of the data.
- No <script>, event handlers, external images, fonts or links. Nothing outside the fence.

## 7. Structural diagrams in SVG — lanes, rows, and a collision pass
Sequence and flow diagrams fail in SVG for one reason: coordinates chosen by eye. Compute them.
- **Lanes.** With N participants over an inner width W (left margin 32, right margin 32), lane width is `L = W / N` and lane center `x_i = 32 + L × (i + 0.5)`. The header is a pictogram (§8) centered on `x_i` spanning y 8–28 and the participant label — 12 units, `text-anchor="middle"`, `0.85` tone — at y 44; without a pictogram the label sits at y 28. Lifelines are 1-unit currentColor at `0.28`, drawn exactly at `x_i` from the header bottom + 8 to `H − 32`.
- **Rows.** One event per row with a fixed pitch of 40, the first row 40 below the header bottom: with pictograms `y_k = 96 + 40k`, without them `y_k = 80 + 40k`. Messages, notes and the closing takeaway all take rows; nothing sits between rows and no two events share one. `H = y_last + 40`.
- **Messages.** A line from `x_a` to `x_b` on the row's y: solid for requests, `stroke-dasharray="4 4"` for responses, 1 unit, `0.62` tone. The arrowhead is a 7-unit chevron at the target end in the same tone. The label is centered on the midpoint (`(x_a + x_b) / 2`, `y − 8`) with `text-anchor="middle"`, 11.5 units, `0.62`. If the label is wider than `|x_b − x_a| − 16`, wrap it to two tspans or shorten it — it must never overhang a neighboring lane.
- **Notes.** A note over lanes i…j spans `x_i − L/2 + 8` to `x_j + L/2 − 8`, height 28 for one line or 44 for two, radius 4, filled `currentColor` at `0.04` (or the contrast hue at `0.08` for the one highlighted note), stroke `0.28`, text centered at 11.5. It occupies its own row. **Every lifeline the note crosses is broken around it** (§3): that lane's line stops 8 units above the note and resumes 8 units below, so no stroke runs behind the text and nothing opaque is needed. A note over a single lane is centered on that lane; if its text needs more than `L − 16`, widen it symmetrically around `x_i` — the neighbouring lifelines it now crosses are broken the same way — never shift it off-center to avoid them.
- **Highlight.** Exactly one box carries the insight: stroke `#ea580c` at 1.5 units, fill `#ea580c` at `0.08` (a tint, like every other box), text stays currentColor at `0.9`. Its takeaway sentence sits alone in the last row, 12 units at weight 600, left-aligned to the first lane center.
- **Collision pass before emitting.** Check every label against its span, **every box against the lifelines crossing it — each of those lines must be broken, never painted over**, every arrow end against its lane center, and every row against the 40 pitch. Fix the coordinates; do not leave the gap, the overlap or the empty band. Arrows additionally follow §10.

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

## 9. Geometry-driven diagrams — when the shape itself is the subject
Some figures cannot be redrawn as boxes and arrows without losing what they say: a hexagonal grid where the point is that every neighbour is equidistant, a cell that splits into seven smaller cells, a coverage that shrinks when children merge into a parent, the same shape before and after an operation. Draw these, and draw them by computing every coordinate.

- **Compute, never eyeball.** Derive each centre and vertex from the lattice's formula, exactly as §7 derives lanes and rows. A hand-placed hexagon that is 3 units off reads as a drawing mistake and undoes the figure's authority.
- **Define the shape once, place it many times.** Put the repeated cell in `<defs>` as `<path id="…" d="…"/>` and place every instance with `<use href="#…" x="…" y="…"/>` — `x`/`y` translate it, and a per-instance `fill-opacity` picks that cell out of the crowd. Never paste fifty near-identical paths. **Ids are document-wide**: every figure in one note shares a page, so prefix them per figure (`f1hex`, `f2child`, `f3ring`) or one `<defs>` silently wins over another.
- **Pointy-top hexagon, circumradius `R`** (vertical left and right edges, vertex up and down — the orientation H3 illustrations use):
  - path: `M 0,-R L a,-R/2 L a,R/2 L 0,R L -a,R/2 L -a,-R/2 Z` with `a = √3·R/2`
  - axial `(q, r)` to centre: `x = cx + √3·R·(q + r/2)`, `y = cy + 1.5·R·r`
  - the k-ring (a cell plus k rings around it) is every `(q, r)` with `max(|q|, |r|, |q+r|) ≤ k`, and it holds `3k² + 3k + 1` cells
  - a parent that covers seven children has `R_parent = R_child · √7`; nest the same rosette inside each child to show the next level down
- **Panels for a comparison.** Split the canvas into equal panels (before/after, condition met/not met), give each a 12-unit `0.62` caption above and a `0.62` result line below, and separate them with one vertical hairline at `0.12`. The arrow between two clusters goes **strictly in the gap between their bounding boxes** — compute both boxes first, then place it. A cluster's labels sit outside its bounding box; nothing is written on top of the shapes except a single 600-weight value inside the one cell being pointed at.
- **Fan-out routing.** One source to N targets is drawn orthogonally, never as N diagonals: a trunk from the source to a spine `x`, one vertical spine spanning the first and last target rows, then one horizontal branch per target with the chevron at the target's edge. Every endpoint is an anchor and every branch lands on its target's centre — §10 is binding here too.
- **What stays on the 8-unit grid.** Margins, panel anchors, caption baselines, legend rows and the canvas height. Vertices, radii and lattice centres follow the geometry — rounding them to 8 would break the tiling. Derive `H` from the tallest cluster's bounding box plus the caption rows.
- **Collision pass before emitting.** Check the rightmost and bottommost extent of every cluster against the margins, each label against the bounding box it must clear, each arrow against both gaps, and — as in §3 — break any reference line that would otherwise run behind text. A line that must stay visible over a cluster (a cutoff, a boundary) is drawn **after** the shapes, and still broken around the label.
- **Do not let the picture assert what the thing does not do.** An idealised lattice — children drawn as exactly filling their parent when the real scheme only approximates it — is fine as a teaching drawing, but the note must say so in a sentence or a `[!WARNING]` next to the figure. A figure that quietly overstates is worse than no figure.

## 10. Connectors — anchors, stops and routing
Most broken diagrams are not broken drawings; they are arrows attached to coordinates nobody computed. An arrow that leaves a box's edge at one height and arrives at a row sitting at another height reads as a rendering bug even when every shape is perfect. Fix it the same way §7 fixes lanes: by naming the points first.

- **Every shape you connect gets four anchors, written down before any arrow.** For a shape at `(x, y, w, h)`: `L = (x, y + h/2)`, `R = (x + w, y + h/2)`, `T = (x + w/2, y)`, `B = (x + w/2, y + h)`. An arrow's two endpoints are anchors, never anything else. If you are about to type a y that is not some shape's `y + h/2`, stop — you are eyeballing.
- **Attach to the thing you mean, not to its container.** When the target is a row, a pill or a card inside a group box, the anchor is that inner shape's, not the group's. The arrow may cross the group's border on the way in; that is normal and reads as entering the group.
- **Align the pair, do not slant to cover the gap.** Connecting N sources to N targets means the k-th source's `R` and the k-th target's `L` share one y. If they do not, **move one of the two stacks** so they do. A slanted arrow used to bridge a 16-unit mismatch is the single most common defect in these diagrams.
- **One source to many targets, or many to one, goes through a spine** (§9): leave the single shape's anchor, run to a spine placed in the gap, run the spine across the targets' anchors, then branch. Never leave one box's edge at several different heights — that reads as arrows sliding along the border.
- **Route orthogonally with clearance.** Segments are horizontal or vertical only; a diagonal belongs to a §9 cycle and nothing else. When anchors are not aligned, bend once or twice and put every turn in the **gap between bounding boxes**, at least 16 units clear of both. A segment never passes through a shape it is not ending on.
- **The head's tip is the anchor.** The line stops exactly at the target's edge — not short of it, not past it, never inside the border. Chevrons are 7 units and follow the **last** segment's direction, not the connector's overall direction:
  - right, ending at `(x, y)`: `m x−7,y−6 l 7,6 l −7,6`
  - left: `m x+7,y−6 l −7,6 l 7,6`
  - down: `m x−6,y−7 l 6,7 l 6,−7`
  - up: `m x−6,y+7 l 6,−7 l 6,7`
- **A feedback loop goes around everything, not between things.** A return path leaves a `B` or `R` anchor, travels in a corridor that clears every bounding box it passes by at least 16 units, and enters a `T` or `B` anchor. Label it inside that corridor and break the line around the label (§3). A loop that starts at a box's corner instead of an anchor is the giveaway that it was drawn by hand.
- **Reference lines are not connectors.** A cutoff marker, a mean line, a lattice spoke or a sequence lifeline may cross shapes; it has no head and obeys §3 (break it around text) rather than these rules.
- **Collision pass for connectors, before emitting.** For each arrow: both endpoints equal a declared anchor; each endpoint's coordinate matches the centre of the shape it touches; no segment crosses a shape other than at its own endpoint or a group border; every turn sits in a gap; the chevron matches the last segment's direction.

## 11. Intervals and outliers — when a bar is the wrong mark
A bar says "this much, measured from zero". Plenty of the numbers worth drawing do not mean that, and forcing them into a bar produces a chart that is unreadable, misleading, or both.

- **A range is not a bar.** A min–max span marks two *positions*, not a magnitude. Drawn as a rounded rect growing from the axis, a short span reads as a bar that got cut off. Draw it as a capped range instead: a 3-unit line between the two values, a 1.5-unit cap 14 units tall at each end, and the value label after the far cap. The caps are what tell the reader "this is an interval".
- **Which axis may start above zero.** A bar's axis must include zero, because the bar's length *is* the value (§3). A position mark — a range, a dot plot, a timeline span — measures where something sits, so its axis may start anywhere the data needs. State the span in the axis title so nobody has to guess.
- **One outlier must not set the scale for everyone.** When the largest value is several times the rest, scaling to it squeezes every other mark into a stub and the chart stops answering its own question. Scale to the readable majority, run the outlier out to the axis boundary and finish it with a chevron instead of a cap, keep its real number in its label, and say it overflows in the single 600-weight note.
- **Minimum legible size: 16 units.** A mark narrower than that carries nothing the value label did not already say. If a mark comes out shorter, the scale is wrong or the mark is wrong — fix one of them. The exception is a value that really is near zero next to its peers; there the stub *is* the finding, and the label carries it.
