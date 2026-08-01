# Loremetry — Report Reference

This guide explains every report you can run from the Analyzer, how saved reports are stored and viewed, and where to manage supporting story data.

---

## How platforms work

Use the tabs at the top of the Analyzer to switch context:

| Tab | Purpose |
|---|---|
| **KDP/Wide** | Amazon KDP and wide distribution — KDP Analysis, Wide Analysis, keywords, competition |
| **Craft** | Structural and prose craft audits |
| **Publish** | Reader experience, polish, and formatter prep |
| **Saved Reports** | Read or delete reports you have already generated for the active story |

On **KDP/Wide**, **Craft**, and **Publish**, select reports with the checkboxes and run them. On **Saved Reports**, click a report name to open it.

Dependencies: some reports need others first (shown in the Analyzer). Selecting a report auto-selects its dependencies when required.

---

## Saved reports

When you run a report, Loremetry stores a snapshot for that story. Your **current** copy of each report type is listed on the **Saved Reports** tab as a flat list of report names — one row per report, no expandable groups.

- **Open** — click the report name.
- **Delete** — click **×** on the row. This removes only that saved snapshot; it does not delete underlying analysis data.
- **Date** — the generated date is shown inside the report when you open it, not in the list.

The report viewer header shows the report name only. Use **Copy**, **Delete**, or **Close** from the toolbar.

### Freshness on the Analyzer

Each report card on KDP/Wide / Craft / Publish shows a status:

| Status | Meaning |
|---|---|
| **has run** | Current saved report matches the manuscript |
| **stale — re-run to refresh** | The manuscript changed since this report was saved; re-run to replace it |
| *(no badge)* | No saved report yet for this type |

When you edit manuscript chapters, older saved snapshots are **archived** automatically (see below). The Analyzer then shows affected reports as stale until you re-run them.

### Archived reports

When the manuscript changes, Loremetry archives the previous current report for each affected type. Archived copies are not shown on **Saved Reports**.

Open **Settings → Archived Reports** to browse them in a table. Use **Read** to open a snapshot or **Delete** to remove one row. The table shows the report name, when it was archived, and the reason (typically a manuscript change).

### Chapter summaries (Story Data)

**Chapter Summaries** are no longer listed as a report you open from Saved Reports. They are **story infrastructure**: per-chapter AI genre-signal summaries that other reports depend on.

Manage them in **Settings → Story Data**:

- View summary status and preview per chapter
- **Refresh summaries** after you edit chapters (uses AI — one call per changed chapter, up to 2000 words each)
- **Clear summaries** if you need to reset

The Analyzer shows summary status before you run reports that depend on them and can prompt you to refresh when needed.

---

## Shared infrastructure

### Chapter summaries

Reads each manuscript chapter (up to 2000 words) and uses AI to extract **genre signals** — setting, tone, themes, conflict type, romance/faith/supernatural markers, pacing, and tropes — not a plot synopsis.

This is the foundation for KDP Analysis, BISAC, discovery keywords, and other positioning reports. Refresh summaries in **Settings → Story Data** (or from the Analyzer prompt) when chapters change. Re-run dependent reports after a manuscript edit so saved results stay current.

---

## KDP only

### KDP Analysis

One report for Amazon Kindle and/or paperback listing metadata:

- **Genre classification** — industry ebook/print labels, master-list ranking scores, comparable titles, reader demographic, and marketing notes
- **KDP categories** — Kindle eBook and/or paperback browse paths (from your catalog / WinningCat data)
- **Print BISAC** — subject codes for KDP paperback (when Print is selected)
- **KDP keywords** — exactly seven keyword strings (≤50 characters each), shared across ebook and print
- **Paste block** — categories, keywords, print BISAC, and description snippet (if Blurb Builder has been run)

Depends on chapter summaries. Optionally uses **Search Terms** and **Keyword Search Results** for measured Amazon demand.

### Search Terms

Short Amazon-style phrases (about 2–4 words) for competition research — what a reader might type to find books like yours.

### Keyword Search Results

Live or stored Amazon keyword volume / competition data (DataForSEO or Canopy), used to ground keyword strategy in measured demand.

### Competition Analysis

AI market landscape from competitor book data: niche competitiveness, books to study, pricing, and a viability verdict for a debut.

### Reader Review Intelligence

Mines competitor reviews for what readers love and hate, their language, gap opportunities, and positioning advice.

### Competitor Author Analysis

Looks at competitor catalogs: release cadence, pricing patterns, series vs standalone, review performance, and strategic takeaways.

### Wide Analysis - Wide

One report for non-Amazon wide ebook and/or print distribution:

- **BISAC** — ebook and/or print subject codes for Ingram, Apple, Kobo, and aggregators
- **Discovery keywords** — ten wide-store SEO phrases (ebook)
- **Google keyword search** — measured Google volume when DataForSEO is configured (ebook)
- **Content & maturity advisory** — age ratings and platform notes (ebook)
- **Wide paste sheet** — copy-ready metadata for Draft2Digital, PublishDrive, IngramSpark, etc.

Depends on chapter summaries.

---

## Craft

### Zeigarnik Effect

**No AI.** Heuristic scan for open loops: cliffhanger endings, unresolved questions, and threads that reappear after a gap. Uses tunable phrase lists in the database. Good for spotting where tension is held open for the reader.

### Continuity Check

AI extraction of continuity-relevant facts per chapter, then judgment of contradictions (manuscript or **series** scope). Suggest-fix can propose rewrites for a flagged contradiction.

### Show Don't Tell

AI flags passages that state emotion or judgment instead of dramatizing it, with severity and context. Suggest-fix offers show-instead rewrites.

### AI-isms

AI flags prose that often reads as machine-generated (stock vocabulary, template antithesis, vague abstraction, etc.). Suggest-fix offers more human rewrites.

### Chekhov's Gun

Finds significant early setups (objects, skills, promises, characters) and checks whether they pay off later. Flags unresolved or unearned payoffs.

### Red Herring vs Abandoned

Separates intentional misdirection from plot threads that look dropped by accident.

### Foreshadowing & Twist Fairness

Checks whether foreshadowing is distributed fairly and whether twists feel earned (surprised, not cheated) or overly telegraphed.

### MacGuffin Clarity

Evaluates whether the driving object or goal is clearly established and consistently motivates character action.

### Want vs Need

For major characters: clear external want vs internal need, and whether that tension drives growth.

### Thematic Throughline

Traces the central theme across scenes, subplots, and arcs — inconsistency, absence, or heavy-handedness.

### Mirror/Foil Characters

Identifies reflect/contrast pairings and whether they illuminate theme and deepen characterization.

### POV Discipline

Flags unintentional POV shifts, head-hopping, and information leaks that break the chosen perspective.

### Story Beat Placement

Maps structural beats against common frameworks (as a reference, not a rigid rule). Highlights beats that are early, late, or missing.

### Scene/Sequel Balance

Action vs reflective (sequel) passages — stretches that exhaust the reader or stall momentum.

### Timeline / Flashback

Whether timeline shifts and flashbacks clarify or confuse, and whether transitions serve a clear purpose.

### Dramatic Irony

Moments where the reader knows more than the characters — and whether that gap creates the intended tension, humor, or dread.

### Stakes Escalation

How stakes rise across the arc; plateaus or reversals that may cause disengagement.

### Cross-Book Setup/Payoff

**Series scope.** Setups planted in earlier books that should pay off later; series-spanning loose ends.

### Series Pacing Comparator

**Series scope.** Compares pacing across installments relative to overall series rhythm.

### Recurring Motif/Theme (Series)

**Series scope.** Motifs and themes across books — cohesion and intentional evolution vs abandoned or contradictory threads.

---

## Publish

### Blurb Builder

Three Amazon/back-cover description variants plus short-form and BookBub one-liners. Run before KDP Analysis to include a description snippet in the KDP paste block.

### Print Production

**No AI.** Estimates page count, trim size, spine width, and paper type from manuscript word count. Includes Ingram and KDP Print checklists.

### AI Beta Reader

Chapter-by-chapter “reader” reactions: engagement score, put-down risk, highlights, and friction. Uses a stronger model slot by default.

### Cliffhanger Score

Scores each chapter ending for pull into the next (cliffhanger / hook / soft landing / resolved). Related to Zeigarnik, but focused on ending pull scores rather than open-loop inventory.

### Hook Strength

Evaluates whether a browsing reader would keep going past page one: intrigue, voice, stakes, clarity, momentum.

### Pacing Curve

Per-chapter pace and drag-risk scores so you can see where the manuscript slows before reviewers tell you.

### Line-level Polish

**No AI.** Heuristic scan for filter words, nearby word echoes, -ly adverbs, and rough passive constructions. Unlimited free runs.

### Vellum & Atticus Prep

**No AI.** Builds a cleaned Markdown manuscript for import into Vellum or Atticus. Convert to `.docx` if your formatter prefers that.

---

## Tips

1. Start publishing work with **chapter summaries** (Settings → Story Data), then genre/ranking, then platform-specific reports.
2. After editing the manuscript, check **Saved Reports** for stale items or open **Settings → Archived Reports** to read older snapshots.
3. Craft and Publish do not require chapter summaries for every report, but a story bible (and Characters / Locations folders) improves AI context when present.
4. Series craft audits need a series with books in reading order and the series scope selected in the Analyzer.
5. Cost estimates in the Analyzer use model prices from Settings; Free means no LLM call (Zeigarnik, Line Polish, Vellum prep).
