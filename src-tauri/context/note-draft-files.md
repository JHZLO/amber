# Deliverable: files in the draft folder, not printed text

This request rewrites or writes a whole note. A single printed answer has an output limit and long notes get cut mid-sentence, so the note is delivered as **files** instead. The input ends with a "[초안 폴더]" section naming an empty directory you may write to.

- Write one file per top-level section (each `# …` block) in reading order, named `01.md`, `02.md`, `03.md` …. Anything that comes before the first `#` (the note's opening lines) goes in `00.md`. The app joins the files in name order with a blank line between them, so the split is invisible to the reader.
- Each file is plain GFM markdown exactly as it should appear in the note — no code fence around the file, no file-name heading, no commentary.
- Create each file with a single Write call. Keep a file under about 6,000 characters; if a section is longer, split it into `03a.md`, `03b.md`, … so no single write is long. Never rewrite the whole note into one file.
- Do not read, create or modify anything outside the draft folder except the reference folders you were given (read-only).
- Do not print the note in your reply. When every file is written, reply with exactly one line: `DONE <number of files>`.
- Everything else in this prompt — structure, heading hierarchy, style, SVG rules, output language — applies to the file contents unchanged.
