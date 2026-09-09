import { describe, expect, it } from "vitest";
import { displaySubject, renderChangelog, summarize, syncHtml } from "./site-sync.mjs";

const PAGE = `<p class="eyebrow mono">Local-first · <span data-latest>v0.1.0</span></p>
<a class="btn magnetic" data-tarball href="https://github.com/JHZLO/amber/archive/refs/tags/v0.1.0.tar.gz">Get the source <span class="mono ver" data-latest>v0.1.0</span></a>
<p class="reveal" id="tagline">Tags since 2026-07-16 · <a href="https://github.com/JHZLO/amber/tags">all tags</a></p>
      <ol>
        <!-- changelog:start -->
        <li>old</li>
        <!-- changelog:end -->
      </ol>`;

describe("site-sync", () => {
  it("strips conventional prefixes and capitalizes", () => {
    expect(displaySubject("feat(notes): run whole-note AI writing in the background")).toBe("Run whole-note AI writing in the background");
    expect(displaySubject("fix!: keep the first half")).toBe("Keep the first half");
    expect(displaySubject("Plain subject")).toBe("Plain subject");
  });

  it("summarizes a tag from its commit subjects, skipping bumps", () => {
    expect(summarize(["build: Bump version to v0.20.18", "feat(notes): Run it", "fix(ai): Keep it", "docs: Old"])).toBe("Run it · Keep it");
    expect(summarize(["build: Bump version to v0.20.19"])).toBe("Maintenance release");
  });

  it("rewrites only the marked spots and is idempotent", () => {
    const entries = [
      { tag: "v0.20.19", date: "2026-09-10", text: "Refresh the landing page from git" },
      { tag: "v0.20.18", date: "2026-09-09", text: "Run whole-note AI writing in the background" },
    ];
    const once = syncHtml(PAGE, { entries, tagCount: 101, since: "2026-07-16" });
    expect(once).toContain(">v0.20.19</span>");
    expect(once).not.toContain(">v0.1.0<");
    expect(once).toContain('data-tarball href="https://github.com/JHZLO/amber/archive/refs/tags/v0.20.19.tar.gz"');
    expect(once).toContain('id="tagline">101 tags since 2026-07-16 · <a');
    expect(once).not.toContain("<li>old</li>");
    expect(once).toContain('class="layer reveal now"><span class="mono tag">v0.20.19</span>');
    expect(once).toContain("<p>Refresh the landing page from git.</p>");
    expect(once).toContain("<!-- changelog:end -->\n      </ol>");
    expect(syncHtml(once, { entries, tagCount: 101, since: "2026-07-16" })).toBe(once);
  });

  it("escapes HTML in subjects", () => {
    expect(renderChangelog([{ tag: "v1.0.0", date: "2027-01-01", text: "Use <b> & \"quotes\"" }])).toContain("Use &lt;b&gt; &amp; &quot;quotes&quot;.");
  });
});
