// reportRenderer.ts — Renders structured JSON report data to HTML.
// Each schema maps to a template function. Legacy markdown gets a fallback.

interface ReportEnvelope {
  doc_type: string;
  label: string;
  format: string;   // "json" | "markdown"
  content: string;
  generated_at: string;
}

// ── Public API ────────────────────────────────────────────────────────────────

export function renderReport(envelope: ReportEnvelope, storyName: string): string {
  const timestamp = new Date(envelope.generated_at).toLocaleString();
  const header = `
    <div class="report-header">
      <h1>${esc(storyName)}</h1>
      <h2>${esc(envelope.label)}</h2>
      <div class="report-meta">Generated: ${esc(timestamp)}</div>
    </div>
  `;

  let body: string;
  if (envelope.format === 'json') {
    const data = JSON.parse(envelope.content);
    body = renderBySchema(data, envelope.doc_type);
  } else {
    // Legacy markdown fallback — wrap in a simple pre/formatted block
    body = `<div class="report-markdown">${markdownToHtml(envelope.content)}</div>`;
  }

  return header + body;
}

// ── Schema dispatcher ─────────────────────────────────────────────────────────

function renderBySchema(data: any, docType: string): string {
  const schema = data.schema || '';

  if (schema === 'kdp_keywords_v1') return renderKdpKeywords(data);
  if (schema === 'mi_search_terms_v1') return renderSearchTerms(data);
  if (schema === 'genre_analysis_v1') return renderGenreAnalysis(data);
  if (schema === 'full_report_v1') return renderFullReport(data);
  if (schema === 'category_finder_v1') return renderCategoryFinder(data);
  if (schema === 'category_finder_standalone_v1') return renderCategoryFinderStandalone(data);
  if (schema === 'competition_report_v1') return renderCompetitionReport(data);
  if (schema === 'review_mining_v1') return renderReviewMining(data);
  if (schema === 'author_analysis_v1') return renderAuthorAnalysis(data);
  if (schema === 'analysis_v1') return renderCombinedAnalysis(data);
  if (schema === 'kdp_paste_v1') return renderKdpPaste(data);
  if (schema === 'chapter_summaries_v1') return renderChapterSummaries(data);
  if (schema === 'keyword_search_v1') return renderKeywordSearch(data);
  if (schema === 'genre_ranking_v1') return renderGenreRanking(data);
  if (schema === 'discovery_keywords_v1') return renderDiscoveryKeywords(data);
  if (schema === 'activity_log_v1') return renderActivityLog(data);
  if (schema === 'zeigarnik_v1') return renderZeigarnik(data);
  if (schema === 'readability_v1') return renderReadability(data);
  if (schema === 'continuity_v1') return renderContinuity(data);
  if (schema === 'show_dont_tell_v1') return renderShowDontTell(data);
  if (schema === 'ai_isms_v1') return renderAiIsms(data);
  if (schema === 'craft_audit_v1') return renderCraftAudit(data);
  if (schema === 'ai_beta_reader_v1') return renderAiBetaReader(data);
  if (schema === 'cliffhanger_score_v1') return renderCliffhangerScore(data);
  if (schema === 'hook_strength_v1') return renderHookStrength(data);
  if (schema === 'pacing_curve_v1') return renderPacingCurve(data);
  if (schema === 'line_polish_v1') return renderLinePolish(data);
  if (schema === 'blurb_builder_v1') return renderBlurbBuilder(data);
  if (schema === 'print_production_v1') return renderPrintProduction(data);
  if (schema === 'vellum_prep_v1') return renderVellumPrep(data);

  // Detect BISAC classification by structure or doc_type
  if (docType === 'bisac_classification' || (data.ebook && Array.isArray(data.ebook))) {
    return renderBisacSection(data);
  }

  // Unknown schema — dump as formatted JSON
  return `<pre class="report-json">${esc(JSON.stringify(data, null, 2))}</pre>`;
}

// ── KDP Keywords ──────────────────────────────────────────────────────────────

function renderKdpKeywords(data: any): string {
  const entries = data.entries || [];
  const strategy = data.strategy || '';
  const sourceNote = data.source_note || '';

  let html = '';
  if (sourceNote) html += `<p class="report-note">${esc(sourceNote)}</p>`;

  html += `
    <section class="report-section">
      <h3>Your 7 KDP Keyword Strings</h3>
      <p class="report-hint">Ready to paste directly into KDP's keyword fields. Each must be 50 characters or fewer.</p>
      <table class="report-table">
        <thead><tr><th>#</th><th>Keyword String</th><th>Chars</th><th>Rationale</th></tr></thead>
        <tbody>
  `;
  for (const e of entries) {
    const overClass = e.over_limit ? ' class="over-limit"' : '';
    html += `<tr${overClass}>
      <td>${e.field}</td>
      <td class="keyword-cell">${esc(e.string)}</td>
      <td>${e.chars}</td>
      <td>${esc(e.rationale)}</td>
    </tr>`;
  }
  html += `</tbody></table></section>`;

  if (strategy) {
    html += `
      <section class="report-section">
        <h3>Strategy</h3>
        <p>${esc(strategy)}</p>
      </section>
    `;
  }

  html += `
    <section class="report-section">
      <h3>How to Use</h3>
      <ol>
        <li>Go to KDP &rarr; Your Books &rarr; Edit eBook Details</li>
        <li>Scroll to <strong>Keywords</strong> (7 fields)</li>
        <li>Paste one string per field</li>
        <li>Do NOT use commas inside a field</li>
        <li>Do NOT repeat words already in your title, subtitle, or categories</li>
      </ol>
    </section>
  `;

  return html;
}

// ── Search Terms ───────────────────────────────────────────────────────────────

function renderSearchTerms(data: any): string {
  const keywords: string[] = data.keywords || [];

  let html = `
    <section class="report-section">
      <h3>Competition Search Terms</h3>
      <p class="report-hint">These short phrases are used for competition analysis — finding competing books in the same niche on Amazon. They are NOT the same as your KDP keyword strings.</p>
      <table class="report-table">
        <thead><tr><th>#</th><th>Search Phrase</th></tr></thead>
        <tbody>
  `;
  keywords.forEach((kw, i) => {
    html += `<tr><td>${i + 1}</td><td class="keyword-cell">${esc(kw)}</td></tr>`;
  });
  html += `</tbody></table></section>`;

  return html;
}

// ── Genre Analysis ────────────────────────────────────────────────────────────

function renderGenreAnalysis(data: any): string {
  let html = `<p class="report-note">Classifications are based on AI training data. Verify KDP paths via Canopy or Amazon.</p>`;

  // Industry classification
  html += `<section class="report-section"><h3>Industry Genre Classification</h3>`;
  html += `<div class="two-col">`;
  html += `<div><h4>Ebook</h4><p class="genre-primary">${esc(data.industry_ebook)}</p>`;
  if (data.comps_ebook?.length) {
    html += `<p><strong>Comparable titles:</strong></p><ul>`;
    for (const c of data.comps_ebook) html += `<li>${esc(c)}</li>`;
    html += `</ul>`;
  }
  html += `<p><strong>Reader demographic:</strong> ${esc(data.reader_demographic)}</p></div>`;
  html += `<div><h4>Print</h4><p class="genre-primary">${esc(data.industry_print)}</p>`;
  html += `<p><strong>Bookstore shelving:</strong> ${esc(data.bookstore_shelving)}</p>`;
  if (data.comps_print?.length) {
    html += `<p><strong>Comparable titles:</strong></p><ul>`;
    for (const c of data.comps_print) html += `<li>${esc(c)}</li>`;
    html += `</ul>`;
  }
  html += `</div></div></section>`;

  // KDP Categories
  html += `<section class="report-section"><h3>KDP Category Recommendations</h3>`;
  html += `<div class="two-col">`;
  html += `<div><h4>Kindle Ebook</h4><ul class="category-list">`;
  for (const p of data.kdp_ebook || []) html += `<li><code>${esc(p)}</code></li>`;
  html += `</ul></div>`;
  html += `<div><h4>KDP Print</h4><ul class="category-list">`;
  for (const p of data.kdp_print || []) html += `<li><code>${esc(p)}</code></li>`;
  html += `</ul></div></div></section>`;

  // Genre Signals
  if (data.genre_signals) {
    html += `<section class="report-section"><h3>Genre Signals Summary</h3><p>${esc(data.genre_signals)}</p></section>`;
  }

  // Marketing Notes
  if (data.marketing_notes?.length) {
    html += `<section class="report-section"><h3>Marketing Notes</h3><ul>`;
    for (const n of data.marketing_notes) html += `<li>${esc(n)}</li>`;
    html += `</ul></section>`;
  }

  return html;
}

// ── Full Report ───────────────────────────────────────────────────────────────

function renderFullReport(data: any): string {
  let html = '';
  if (data.genre_analysis) {
    html += renderGenreAnalysis(data.genre_analysis);
  }
  if (!data.competition_done) {
    html += `<p class="report-note">Run <strong>Analyze Competition</strong> to add market data.</p>`;
  }
  return html;
}

// ── Category Finder (story-bound) ─────────────────────────────────────────────

function renderCategoryFinder(data: any): string {
  let html = '';
  if (data.method) {
    html += `<p class="report-hint">${esc(data.method)}</p>`;
  }

  const stores: any[] = data.stores || [];
  for (const store of stores) {
    html += renderStoreSection(store);
  }
  return html;
}

function renderStoreSection(store: any): string {
  let html = `<section class="report-section"><h3>${esc(store.store)}</h3>`;

  // Per-genre results
  const perGenre: any[] = store.per_genre || [];
  if (perGenre.length) {
    html += `<h4>Per-Genre Results</h4>`;
    for (const g of perGenre) {
      html += `<div class="genre-block"><strong>${esc(g.genre)} (${g.confidence}%)</strong>`;
      if (!g.picks?.length) {
        html += `<p class="muted">No confident catalog match.</p>`;
      } else {
        html += `<ul>`;
        for (const p of g.picks) {
          html += `<li><code>${esc(p.path)}</code> (${p.confidence}% match) — ${esc(p.reason)}</li>`;
        }
        html += `</ul>`;
      }
      html += `</div>`;
    }
  }

  // Final categories
  const finals: any[] = store.final_categories || [];
  if (finals.length) {
    html += `<h4>Best for Discoverability</h4>`;
    html += `<table class="report-table"><thead><tr>
      <th>#</th><th>Category Path</th><th>Fit</th><th>Sales to #10</th><th>Publisher</th><th>KU</th><th>Matched By</th>
    </tr></thead><tbody>`;
    for (const q of finals) {
      const bonus = q.is_bonus ? ' <span class="badge">bonus</span>' : '';
      const verified = q.verified
        ? `${esc(q.sales_to_ten)}`
        : '<span class="muted">unverified</span>';
      html += `<tr>
        <td>${q.rank}${bonus}</td>
        <td><code>${esc(q.path)}</code></td>
        <td>${q.fit_confidence}%</td>
        <td>${verified}</td>
        <td>${q.verified ? esc(q.publisher_pct) : '—'}</td>
        <td>${q.verified ? esc(q.ku_pct) : '—'}</td>
        <td>${esc((q.agreeing_genres || []).join(', '))}</td>
      </tr>`;
    }
    html += `</tbody></table>`;
  } else {
    html += `<p class="muted">No candidates cleared the fit bar for this store.</p>`;
  }

  html += `</section>`;
  return html;
}

// ── Category Finder Standalone ────────────────────────────────────────────────

function renderCategoryFinderStandalone(data: any): string {
  let html = `<p class="report-hint">Genre: ${esc(data.genre)} | Store: ${esc(data.store)}</p>`;

  const matched: any[] = data.matched || [];
  if (matched.length) {
    html += `<section class="report-section"><h3>Matched Categories</h3>`;
    html += `<table class="report-table"><thead><tr>
      <th>Category Path</th><th>Match</th><th>Sales to #1</th><th>Sales to #10</th><th>Publisher</th><th>KU</th>
    </tr></thead><tbody>`;
    for (const r of matched) {
      html += `<tr>
        <td><code>${esc(r.path)}</code></td><td>${r.confidence}%</td>
        <td>${esc(r.sales_to_one)}</td><td>${esc(r.sales_to_ten)}</td>
        <td>${esc(r.publisher_pct)}</td><td>${esc(r.ku_pct)}</td>
      </tr>`;
    }
    html += `</tbody></table>`;
    // Keywords per matched category
    for (const r of matched) {
      if (r.keywords) {
        html += `<div class="keywords-block"><h4>${esc(r.path)}</h4><pre>${esc(r.keywords)}</pre></div>`;
      }
    }
    html += `</section>`;
  }

  const considered: any[] = data.considered || [];
  if (considered.length) {
    html += `<section class="report-section"><h3>Also Considered (below 80%)</h3><ol>`;
    for (const r of considered) {
      html += `<li>${esc(r.path)} — <strong>${r.confidence}%</strong></li>`;
    }
    html += `</ol></section>`;
  }

  const failed: any[] = data.failed || [];
  if (failed.length) {
    html += `<section class="report-section"><h3>Search Failures</h3><ul>`;
    for (const r of failed) {
      html += `<li>${esc(r.path)}: ${esc(r.note || 'unknown error')}</li>`;
    }
    html += `</ul></section>`;
  }

  return html;
}

// ── Competition Report ────────────────────────────────────────────────────────

function renderCompetitionReport(data: any): string {
  if (data.content_format === 'markdown') {
    return `<div class="report-markdown">${markdownToHtml(data.content)}</div>`;
  }
  return `<pre>${esc(JSON.stringify(data, null, 2))}</pre>`;
}

// ── Review Mining Report ──────────────────────────────────────────────────────

function renderReviewMining(data: any): string {
  let html = '';
  const books: any[] = data.books_analyzed || [];
  if (books.length) {
    html += `<p class="report-hint">Based on ${data.total_reviews || '?'} reviews from ${books.length} competitor book${books.length > 1 ? 's' : ''}:</p>`;
    html += `<ul class="category-list">`;
    for (const b of books) html += `<li>${esc(b.title)}</li>`;
    html += `</ul>`;
  }
  if (data.content_format === 'markdown' && data.content) {
    html += `<div class="report-markdown">${markdownToHtml(data.content)}</div>`;
  }
  return html;
}

// ── Author Catalog Analysis Report ────────────────────────────────────────────

function renderAuthorAnalysis(data: any): string {
  let html = '';
  const authors: any[] = data.authors_analyzed || [];
  if (authors.length) {
    html += `<section class="report-section"><h3>Authors Analyzed</h3>`;
    html += `<table class="report-table"><thead><tr><th>Author</th><th>Books</th></tr></thead><tbody>`;
    for (const a of authors) {
      html += `<tr><td><strong>${esc(a.name)}</strong></td><td>${a.book_count}</td></tr>`;
    }
    html += `</tbody></table></section>`;
  }
  if (data.content_format === 'markdown' && data.content) {
    html += `<div class="report-markdown">${markdownToHtml(data.content)}</div>`;
  }
  return html;
}

// ── Combined Analysis Report ──────────────────────────────────────────────────

function renderCombinedAnalysis(data: any): string {
  const sections = data.sections || {};
  let html = '';

  // KDP Paste section
  if (sections.kdp_paste) {
    try {
      const paste = JSON.parse(sections.kdp_paste);
      html += renderKdpPaste(paste);
    } catch { html += renderRawSection('KDP Paste', sections.kdp_paste); }
  }

  // Genre ranking
  if (sections.genre_ranking) {
    try {
      const ranking = JSON.parse(sections.genre_ranking);
      html += renderGenreRanking(ranking);
    } catch { html += renderRawSection('Genre Ranking', sections.genre_ranking); }
  }

  // KDP Categories
  if (sections.kdp_categories) {
    try {
      const cats = JSON.parse(sections.kdp_categories);
      html += renderKdpCategoriesSection(cats);
    } catch { html += renderRawSection('KDP Categories', sections.kdp_categories); }
  }

  // BISAC
  if (sections.bisac) {
    try {
      const bisac = JSON.parse(sections.bisac);
      html += renderBisacSection(bisac);
    } catch { html += renderRawSection('BISAC Classification', sections.bisac); }
  }

  // KDP Keywords
  if (sections.kdp_keywords) {
    try {
      const kw = JSON.parse(sections.kdp_keywords);
      html += renderKdpKeywords(kw);
    } catch { html += renderRawSection('KDP Keywords', sections.kdp_keywords); }
  }

  // Discovery keywords
  if (sections.discovery_keywords) {
    try {
      const dk = JSON.parse(sections.discovery_keywords);
      html += renderDiscoveryKeywords(dk);
    } catch { html += renderRawSection('Discovery Keywords', sections.discovery_keywords); }
  }

  // Positioning
  if (sections.positioning) {
    try {
      const pos = JSON.parse(sections.positioning);
      html += renderPositioning(pos);
    } catch { html += renderRawSection('Positioning Context', sections.positioning); }
  }

  return html;
}

// ── Genre Ranking Section ─────────────────────────────────────────────────────

function renderGenreRanking(data: any): string {
  const genres: any[] = data.genres || [];
  if (!genres.length) return '';

  let html = `<section class="report-section"><h3>Genre Ranking</h3>`;
  html += `<p class="report-hint">Scored independently — percentages do not sum to 100.</p>`;
  html += `<table class="report-table"><thead><tr><th>Genre</th><th>Confidence</th><th>Reasoning</th></tr></thead><tbody>`;
  for (const g of genres) {
    html += `<tr>
      <td><strong>${esc(g.genre)}</strong></td>
      <td>${g.confidence}%</td>
      <td>${esc(g.reason)}</td>
    </tr>`;
  }
  html += `</tbody></table></section>`;
  return html;
}

// ── KDP Categories Section (combined report) ──────────────────────────────────

function renderKdpCategoriesSection(data: any): string {
  const stores: any[] = data.stores || [];
  if (!stores.length) return '';

  let html = `<section class="report-section"><h3>KDP Categories</h3>`;
  for (const store of stores) {
    html += `<h4>${esc(store.store)}</h4>`;
    if (store.error) {
      html += `<p class="muted">${esc(store.error)}</p>`;
      continue;
    }
    const cats: any[] = store.categories || [];
    if (!cats.length) {
      html += `<p class="muted">No candidates cleared the fit bar for this store.</p>`;
      continue;
    }
    html += `<table class="report-table"><thead><tr>
      <th>#</th><th>Category Path</th><th>Fit</th><th>Sales to #10</th><th>Matched By</th>
    </tr></thead><tbody>`;
    for (const c of cats) {
      const bonus = c.is_bonus ? ' <span class="badge">bonus</span>' : '';
      const sales = c.verified ? esc(c.sales_to_ten) : '<span class="muted">unverified</span>';
      html += `<tr>
        <td>${c.rank}${bonus}</td>
        <td><code>${esc(c.path)}</code></td>
        <td>${c.fit_confidence}%</td>
        <td>${sales}</td>
        <td>${esc((c.agreeing_genres || []).join(', '))}</td>
      </tr>`;
      // Top bestsellers for this category
      const books: any[] = c.top_books || [];
      if (books.length) {
        html += `<tr><td colspan="5" class="top-books-cell">`;
        for (const b of books) {
          const img = b.image_url ? `<img src="${esc(b.image_url)}" height="50" /> ` : '';
          html += `<a href="https://www.amazon.com/dp/${esc(b.asin)}" class="top-book-link">${img}${esc(b.title)}</a>`;
        }
        html += `</td></tr>`;
      }
    }
    html += `</tbody></table>`;
  }
  html += `</section>`;
  return html;
}

// ── BISAC Section ─────────────────────────────────────────────────────────────

function renderBisacSection(data: any): string {
  let html = `<section class="report-section"><h3>BISAC Classification</h3>`;
  html += `<p class="report-hint">Verify against BISG's free lookup (bisg.org/complete-bisac-subject-headings-list) before submitting. Kindle eBook no longer takes BISAC directly on KDP — this matters for KDP Print and wide/Ingram distribution.</p>`;

  const ebook: any[] = data.ebook || [];
  html += `<h4>Ebook</h4>`;
  if (!ebook.length) {
    html += `<p class="muted">No confident BISAC match.</p>`;
  } else {
    html += `<table class="report-table"><thead><tr><th>Code</th><th>Heading</th><th>Confidence</th><th>Reasoning</th></tr></thead><tbody>`;
    for (const b of ebook) {
      html += `<tr><td><code>${esc(b.code)}</code></td><td>${esc(b.heading)}</td><td>${b.confidence}%</td><td>${esc(b.reason || '')}</td></tr>`;
    }
    html += `</tbody></table>`;
  }

  html += `<h4>Print</h4>`;
  if (data.print === 'same_as_ebook') {
    html += `<p class="muted">Same as ebook.</p>`;
  } else {
    const print: any[] = data.print || [];
    if (!print.length) {
      html += `<p class="muted">No confident BISAC match.</p>`;
    } else {
      html += `<table class="report-table"><thead><tr><th>Code</th><th>Heading</th><th>Confidence</th><th>Reasoning</th></tr></thead><tbody>`;
      for (const b of print) {
        html += `<tr><td><code>${esc(b.code)}</code></td><td>${esc(b.heading)}</td><td>${b.confidence}%</td><td>${esc(b.reason || '')}</td></tr>`;
      }
      html += `</tbody></table>`;
    }
  }

  html += `</section>`;
  return html;
}

// ── Discovery Keywords Section ────────────────────────────────────────────────

function renderDiscoveryKeywords(data: any): string {
  const keywords: any[] = data.keywords || [];
  if (!keywords.length) return '';

  let html = `<section class="report-section"><h3>Discovery Keywords</h3>`;
  html += `<p class="report-hint">Optimized for Apple Books, Kobo, Google Play, B&N, BookBub, Goodreads, and SEO.</p>`;
  html += `<table class="report-table"><thead><tr><th>Phrase</th><th>Rationale</th></tr></thead><tbody>`;
  for (const k of keywords) {
    html += `<tr><td class="keyword-cell">${esc(k.phrase)}</td><td>${esc(k.rationale)}</td></tr>`;
  }
  html += `</tbody></table></section>`;
  return html;
}

// ── Positioning Section ───────────────────────────────────────────────────────

function renderPositioning(data: any): string {
  let html = `<section class="report-section"><h3>Positioning Context</h3>`;
  html += `<table class="report-table"><tbody>`;
  html += `<tr><td><strong>Reader demographic</strong></td><td>${esc(data.reader_demographic || '')}</td></tr>`;
  html += `<tr><td><strong>Bookstore shelving</strong></td><td>${esc(data.bookstore_shelving || '')}</td></tr>`;
  html += `</tbody></table>`;

  const compsEbook: string[] = data.comps_ebook || [];
  const compsPrint: string[] = data.comps_print || [];
  if (compsEbook.length || compsPrint.length) {
    html += `<div class="two-col">`;
    if (compsEbook.length) {
      html += `<div><h4>Ebook Comps</h4><ul>`;
      for (const c of compsEbook) html += `<li>${esc(c)}</li>`;
      html += `</ul></div>`;
    }
    if (compsPrint.length) {
      html += `<div><h4>Print Comps</h4><ul>`;
      for (const c of compsPrint) html += `<li>${esc(c)}</li>`;
      html += `</ul></div>`;
    }
    html += `</div>`;
  }

  html += `</section>`;
  return html;
}

// ── KDP Paste Section ─────────────────────────────────────────────────────────

function renderKdpPaste(data: any): string {
  let html = `<section class="report-section"><h3>KDP Metadata &mdash; Ready to Paste</h3>`;

  html += `<div class="two-col">`;
  html += `<div><h4>Categories (Kindle eBook)</h4><ol class="category-list">`;
  for (const c of data.kindle_categories || []) html += `<li><code>${esc(c)}</code></li>`;
  html += `</ol></div>`;
  html += `<div><h4>Categories (Paperback)</h4><ol class="category-list">`;
  for (const c of data.print_categories || []) html += `<li><code>${esc(c)}</code></li>`;
  html += `</ol></div></div>`;

  const keywords: string[] = data.keywords || [];
  if (keywords.length) {
    html += `<h4>Keywords</h4>`;
    html += `<table class="report-table keywords-paste-table"><thead><tr><th>Field 1–4</th><th>Field 5–7</th></tr></thead><tbody>`;
    for (let i = 0; i < 4; i++) {
      const left = keywords[i] || '';
      const right = keywords[i + 4] || '';
      html += `<tr><td>${esc(left)}</td><td>${esc(right)}</td></tr>`;
    }
    html += `</tbody></table>`;
  }

  html += `</section>`;
  return html;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

// ── Chapter Summaries Report ──────────────────────────────────────────────────

function renderChapterSummaries(data: any): string {
  const chapters: any[] = data.chapters || [];
  if (!chapters.length) return '<p class="muted">No chapter summaries available.</p>';

  const totalWords = data.total_words || chapters.reduce((sum: number, c: any) => sum + (c.word_count || 0), 0);
  let html = `<p class="report-hint">${chapters.length} chapters, ${totalWords.toLocaleString()} total words</p>`;
  html += `<table class="report-table"><thead><tr><th>#</th><th>Chapter</th><th>Words</th><th>Genre Signals</th></tr></thead><tbody>`;
  chapters.forEach((ch, i) => {
    html += `<tr>
      <td>${i + 1}</td>
      <td><strong>${esc(ch.title || ch.file)}</strong></td>
      <td>${(ch.word_count || 0).toLocaleString()}</td>
      <td>${esc(ch.signals || '').substring(0, 200)}${(ch.signals || '').length > 200 ? '...' : ''}</td>
    </tr>`;
  });
  html += `</tbody></table>`;
  return html;
}

// ── Keyword Search Results Report ─────────────────────────────────────────────

function renderKeywordSearch(data: any): string {
  const keywords: any[] = data.keywords || [];
  if (!keywords.length) return '<p class="muted">No keyword search results available.</p>';

  let html = `<p class="report-hint">${keywords.length} keywords analyzed</p>`;
  html += `<table class="report-table"><thead><tr><th>Keyword</th><th>Est. Monthly Searches</th><th>Competition</th><th>Est. Earnings</th></tr></thead><tbody>`;
  for (const k of keywords) {
    const compClass = k.competition === 'Low' ? 'class="report-color-success"' :
                      k.competition === 'High' ? 'class="report-color-danger"' : '';
    html += `<tr>
      <td class="keyword-cell">${esc(k.keyword)}</td>
      <td>${esc(k.searches)}</td>
      <td ${compClass}><strong>${esc(k.competition)}</strong></td>
      <td>${esc(k.earnings)}</td>
    </tr>`;
  }
  html += `</tbody></table>`;
  return html;
}

function renderRawSection(title: string, content: string): string {
  // For sections that are still plain text (not yet converted to JSON)
  return `<section class="report-section"><h3>${esc(title)}</h3><pre class="report-raw">${esc(content)}</pre></section>`;
}

/** Minimal markdown to HTML — handles headers, bold, italic, code, lists, hr */
function markdownToHtml(md: string): string {
  return md
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/^### (.+)$/gm, '<h4>$1</h4>')
    .replace(/^## (.+)$/gm, '<h3>$1</h3>')
    .replace(/^# (.+)$/gm, '<h2>$1</h2>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    .replace(/`(.+?)`/g, '<code>$1</code>')
    .replace(/^- (.+)$/gm, '<li>$1</li>')
    .replace(/(<li>.*<\/li>\n?)+/g, (m) => `<ul>${m}</ul>`)
    .replace(/^---$/gm, '<hr>')
    .replace(/^> (.+)$/gm, '<blockquote>$1</blockquote>')
    .replace(/\n{2,}/g, '</p><p>')
    .replace(/^/, '<p>').replace(/$/, '</p>')
    .replace(/<p><\/p>/g, '')
    .replace(/<p>(<h[2-4]>)/g, '$1')
    .replace(/(<\/h[2-4]>)<\/p>/g, '$1')
    .replace(/<p>(<ul>)/g, '$1')
    .replace(/(<\/ul>)<\/p>/g, '$1')
    .replace(/<p>(<hr>)<\/p>/g, '$1')
    .replace(/<p>(<blockquote>)/g, '$1')
    .replace(/(<\/blockquote>)<\/p>/g, '$1');
}

// ── Zeigarnik Effect Report ───────────────────────────────────────────────────

function renderZeigarnik(data: any): string {
  const summary = data.summary || {};
  const chapters: any[] = data.chapters || [];
  const threads: any[] = data.threads || [];

  let html = '';

  // Overall Zeigarnik usage percentage at the top
  const overallPct = summary.overall_zeigarnik_pct ?? 0;
  html += `<div class="zeigarnik-score"><span class="zeigarnik-pct">${overallPct}%</span><span class="zeigarnik-label">Zeigarnik Usage</span></div>`;

  if (data.note) {
    html += `<p class="report-note">${esc(data.note)}</p>`;
  }

  html += `<section class="report-section"><h3>Summary</h3>`;
  html += `<table class="report-table"><tbody>`;
  html += `<tr><td><strong>Chapters analyzed</strong></td><td>${summary.total_chapters ?? '—'}</td></tr>`;
  html += `<tr><td><strong>Total words</strong></td><td>${(summary.total_words ?? 0).toLocaleString()}</td></tr>`;
  html += `<tr><td><strong>Cliffhanger endings</strong></td><td>${summary.cliffhanger_endings ?? 0} of ${summary.total_chapters ?? 0} (${summary.cliffhanger_pct ?? 0}%)</td></tr>`;
  html += `<tr><td><strong>Resolved endings</strong></td><td>${summary.resolved_endings ?? 0}</td></tr>`;
  html += `<tr><td><strong>Open narrative questions</strong></td><td>${summary.total_open_questions ?? 0}</td></tr>`;
  html += `<tr><td><strong>Average tension score</strong></td><td>${summary.avg_tension_score ?? 0} / 100</td></tr>`;
  html += `<tr><td><strong>Candidate open threads</strong></td><td>${summary.open_thread_count ?? 0}</td></tr>`;
  html += `<tr><td><strong>Longest thread gap</strong></td><td>${summary.longest_gap_chapters ?? 0} chapters</td></tr>`;
  html += `</tbody></table></section>`;

  if (threads.length) {
    html += `<section class="report-section"><h3>Open Threads</h3>`;
    html += `<p class="report-hint">A capitalized name, place, or object that appears, then goes quiet for several chapters, then resurfaces — the textual shape of an unresolved thread. Sorted by the longest gap.</p>`;
    html += `<table class="report-table"><thead><tr><th>Term</th><th>First Seen</th><th>Gap</th><th>Resurfaces</th><th>Mentions</th><th>Context</th></tr></thead><tbody>`;
    for (const t of threads) {
      html += `<tr>
        <td><strong>${esc(t.term)}</strong></td>
        <td>Ch. ${(t.first_chapter_index ?? 0) + 1}</td>
        <td>${t.max_gap_chapters} chapters / ${(t.max_gap_words ?? 0).toLocaleString()} words</td>
        <td>Ch. ${(t.gap_end_index ?? 0) + 1}</td>
        <td>${t.mention_count}</td>
        <td class="muted">${esc(t.first_snippet || '')}</td>
      </tr>`;
    }
    html += `</tbody></table></section>`;
  } else {
    html += `<section class="report-section"><p class="muted">No long-gap threads found above the configured threshold.</p></section>`;
  }

  if (chapters.length) {
    html += `<section class="report-section"><h3>Chapter Endings</h3>`;
    html += `<table class="report-table"><thead><tr><th>#</th><th>Chapter</th><th>Words</th><th>Questions</th><th>Ending</th><th>Tension</th></tr></thead><tbody>`;
    chapters.forEach((c: any, i: number) => {
      const endingClass = c.ending_type === 'cliffhanger' ? 'class="report-color-danger"' :
                          c.ending_type === 'resolved' ? 'class="report-color-success"' : '';
      html += `<tr>
        <td>${i + 1}</td>
        <td><strong>${esc(c.title || c.file)}</strong></td>
        <td>${(c.word_count ?? 0).toLocaleString()}</td>
        <td>${c.question_count ?? 0}</td>
        <td ${endingClass}><strong>${esc(c.ending_type)}</strong></td>
        <td>${c.tension_score ?? 0}</td>
      </tr>`;
      if (c.ending_snippet) {
        html += `<tr><td colspan="6" class="top-books-cell muted">"${esc(c.ending_snippet)}"</td></tr>`;
      }
    });
    html += `</tbody></table></section>`;
  }

  return html;
}

// ── Readability Report ────────────────────────────────────────────────────────

function gradeClass(grade: number): string {
  if (grade <= 6) return 'readability-easy';
  if (grade <= 9) return 'readability-mid';
  if (grade <= 12) return 'readability-hard';
  return 'readability-very-hard';
}

function renderReadability(data: any): string {
  const summary = data.summary || {};
  const chapters: any[] = data.chapters || [];

  let html = '';

  const avgGrade = summary.avg_flesch_kincaid_grade ?? 0;
  html += `<div class="zeigarnik-score readability-score ${gradeClass(avgGrade)}">`;
  html += `<span class="zeigarnik-pct">${avgGrade}</span>`;
  html += `<span class="zeigarnik-label">Avg. grade level (Flesch-Kincaid)</span>`;
  html += `</div>`;

  if (data.note) {
    html += `<p class="report-note">${esc(data.note)}</p>`;
  }

  html += `<section class="report-section"><h3>Summary</h3>`;
  html += `<table class="report-table"><tbody>`;
  html += `<tr><td><strong>Chapters analyzed</strong></td><td>${summary.total_chapters ?? '—'}</td></tr>`;
  html += `<tr><td><strong>Total words</strong></td><td>${(summary.total_words ?? 0).toLocaleString()}</td></tr>`;
  html += `<tr><td><strong>Average grade level</strong></td><td>${avgGrade} (${esc(summary.avg_grade_label || '')})</td></tr>`;
  html += `<tr><td><strong>Average reading ease</strong></td><td>${summary.avg_flesch_reading_ease ?? 0} — ${esc(summary.avg_ease_label || '')}</td></tr>`;
  html += `<tr><td><strong>Grade range</strong></td><td>${summary.min_grade ?? 0} – ${summary.max_grade ?? 0} (spread ${summary.grade_spread ?? 0})</td></tr>`;
  if (summary.easiest_chapter) {
    html += `<tr><td><strong>Easiest chapter</strong></td><td>${esc(summary.easiest_chapter)} (grade ${summary.min_grade ?? 0})</td></tr>`;
  }
  if (summary.hardest_chapter) {
    html += `<tr><td><strong>Hardest chapter</strong></td><td>${esc(summary.hardest_chapter)} (grade ${summary.max_grade ?? 0})</td></tr>`;
  }
  html += `</tbody></table></section>`;

  if (chapters.length) {
    html += `<section class="report-section"><h3>Per Chapter</h3>`;
    html += `<p class="report-hint">Flesch-Kincaid grade ≈ US school grade needed to read comfortably. Reading ease: 90+ very easy, 60–70 standard, 30– difficult.</p>`;
    html += `<table class="report-table"><thead><tr><th>#</th><th>Chapter</th><th>Words</th><th>Grade</th><th>Reading ease</th><th>Gunning Fog</th></tr></thead><tbody>`;
    chapters.forEach((c: any, i: number) => {
      const g = c.flesch_kincaid_grade ?? 0;
      html += `<tr>
        <td>${i + 1}</td>
        <td><strong>${esc(c.title || c.file)}</strong></td>
        <td>${(c.word_count ?? 0).toLocaleString()}</td>
        <td class="${gradeClass(g)}"><strong>${g}</strong></td>
        <td>${c.flesch_reading_ease ?? 0}</td>
        <td>${c.gunning_fog ?? 0}</td>
      </tr>`;
    });
    html += `</tbody></table></section>`;
  }

  return html;
}

// ── Continuity Check Report ───────────────────────────────────────────────────

function renderContinuity(data: any): string {
  const summary = data.summary || {};
  const findings: any[] = data.findings || [];

  let html = '';
  if (data.note) {
    html += `<p class="report-note">${esc(data.note)}</p>`;
  }
  if (data.scope === 'series') {
    html += `<p class="report-hint">Series-wide check — facts compared across every book in reading order.</p>`;
  }

  html += `<section class="report-section"><h3>Summary</h3>`;
  html += `<table class="report-table"><tbody>`;
  html += `<tr><td><strong>Total findings</strong></td><td>${summary.total_findings ?? 0}</td></tr>`;
  html += `<tr><td><strong>Contradictions</strong></td><td class="report-color-danger"><strong>${summary.contradictions ?? 0}</strong></td></tr>`;
  html += `<tr><td><strong>Possible issues</strong></td><td class="report-color-warning">${summary.possible ?? 0}</td></tr>`;
  html += `<tr><td><strong>Likely intentional / non-issues</strong></td><td class="muted">${summary.likely_intentional ?? 0}</td></tr>`;
  html += `</tbody></table></section>`;

  if (!findings.length) {
    html += `<section class="report-section"><p class="muted">No candidate contradictions found.</p></section>`;
    return html;
  }

  const verdictLabel: Record<string, string> = {
    contradiction: 'Contradiction',
    possible: 'Possible',
    likely_intentional: 'Likely intentional',
  };
  const verdictClass: Record<string, string> = {
    contradiction: 'verdict-contradiction',
    possible: 'verdict-possible',
    likely_intentional: 'verdict-likely_intentional',
  };

  html += `<section class="report-section"><h3>Findings</h3>`;
  findings.forEach((f: any, idx: number) => {
    const verdictCls = verdictClass[f.verdict] || 'verdict-likely_intentional';
    const label = verdictLabel[f.verdict] || esc(f.verdict);
    html += `<div class="genre-block">`;
    html += `<strong>${esc(f.entity)}</strong> &mdash; ${esc(f.attribute)} `;
    html += `<span class="${verdictCls}">[${label}, ${f.confidence}%]</span>`;
    if (f.verdict === 'contradiction' || f.verdict === 'possible') {
      html += ` <a href="#" class="suggest-fix-link" data-finding-index="${idx}">Suggest fix</a>`;
    }
    html += `<p>${esc(f.explanation)}</p>`;
    const occs: any[] = f.occurrences || [];
    if (occs.length) {
      html += `<table class="report-table"><thead><tr><th>Book</th><th>Chapter</th><th>Value</th><th>Quote</th></tr></thead><tbody>`;
      for (const o of occs) {
        html += `<tr>
          <td>${esc(o.story_name || '')}</td>
          <td>${esc(o.chapter_title || o.file || '')}</td>
          <td>${esc(o.value)}</td>
          <td class="muted">“${esc(o.snippet || '')}”</td>
        </tr>`;
      }
      html += `</tbody></table>`;
    }
    html += `</div>`;
  });
  html += `</section>`;

  return html;
}

// ── Show Don't Tell Report ────────────────────────────────────────────────────

function renderShowDontTell(data: any): string {
  return renderPassageViolations(data, {
    emptyMsg: 'No show-don\'t-tell violations found. Nice work.',
    linkClass: 'suggest-sdt-fix-link',
  });
}

function renderAiIsms(data: any): string {
  return renderPassageViolations(data, {
    emptyMsg: 'No AI-isms found. Nice work.',
    linkClass: 'suggest-ai-isms-fix-link',
  });
}

function renderPassageViolations(data: any, opts: { emptyMsg: string; linkClass: string }): string {
  const summary = data.summary || {};
  const chapters: any[] = data.chapters || [];

  let html = '';
  if (data.note) {
    html += `<p class="report-note">${esc(data.note)}</p>`;
  }

  html += `<section class="report-section"><h3>Summary</h3>`;
  html += `<table class="report-table"><tbody>`;
  html += `<tr><td><strong>Chapters checked</strong></td><td>${summary.chapters_checked ?? 0}</td></tr>`;
  html += `<tr><td><strong>Chapters with flags</strong></td><td>${summary.chapters_with_violations ?? 0}</td></tr>`;
  html += `<tr><td><strong>Total flags</strong></td><td class="report-color-danger"><strong>${summary.total_violations ?? 0}</strong></td></tr>`;
  html += `</tbody></table></section>`;

  if (!chapters.length) {
    html += `<section class="report-section"><p class="muted">${esc(opts.emptyMsg)}</p></section>`;
    return html;
  }

  const severityClass: Record<string, string> = {
    minor: 'severity-minor',
    moderate: 'severity-moderate',
    major: 'severity-major',
  };

  chapters.forEach((ch: any, chIdx: number) => {
    const violations: any[] = ch.violations || [];
    html += `<section class="report-section">`;
    html += `<h3>${esc(ch.title || ch.file)} <span class="muted">(${violations.length})</span></h3>`;

    violations.forEach((v: any, vIdx: number) => {
      const sevCls = severityClass[v.severity] || 'severity-minor';
      html += `<div class="sdt-violation">`;
      html += `<div class="sdt-severity ${sevCls}">${esc(v.severity)} <a href="#" class="${opts.linkClass}" data-chapter-index="${chIdx}" data-violation-index="${vIdx}">Suggest fix</a></div>`;
      html += `<div class="sdt-passage">`;
      if (v.context) {
        const ctx = v.context as string;
        const tell = v.telling_text as string;
        const idx = ctx.indexOf(tell);
        if (idx >= 0) {
          html += `${esc(ctx.substring(0, idx))}<span class="sdt-highlight">${esc(tell)}</span>${esc(ctx.substring(idx + tell.length))}`;
        } else {
          html += `${esc(ctx)} <span class="sdt-highlight">${esc(tell)}</span>`;
        }
      } else {
        html += `<span class="sdt-highlight">${esc(v.telling_text)}</span>`;
      }
      html += `</div>`;
      html += `<div class="sdt-why">${esc(v.why)}</div>`;
      html += `</div>`;
    });

    html += `</section>`;
  });

  return html;
}

function renderActivityLog(data: any): string {
  const lines: any[] = data.lines || [];
  if (!lines.length) return '<p class="muted">No log entries.</p>';

  const timestamp = data.timestamp ? new Date(data.timestamp).toLocaleString() : '';
  let html = '';
  if (timestamp) {
    html += `<p class="report-hint">Recorded: ${esc(timestamp)}</p>`;
  }

  html += `<div class="log-stream">`;
  for (const line of lines) {
    const typeClass = esc(line.type || 'log-info');
    const icon = line.icon || '';
    const text = esc(line.text || '').replace(/`([^`]+)`/g, '<code>$1</code>');
    html += `<div class="log-line ${typeClass}">`;
    if (icon) html += `<span class="log-icon">${esc(icon)}</span>`;
    html += `<span class="log-text">${text}</span></div>`;
  }
  html += `</div>`;
  return html;
}

function renderCraftAudit(data: any): string {
  const summary = data.summary || '';
  const findings: any[] = data.findings || [];
  let html = '';
  if (data.audit_id) {
    html += `<p class="report-hint">Audit: <code>${esc(data.audit_id)}</code></p>`;
  }
  if (summary) {
    html += `<section class="report-section"><h3>Summary</h3><p>${esc(summary)}</p></section>`;
  }
  if (!findings.length) {
    html += `<p class="muted">No material findings.</p>`;
    return html;
  }
  html += `<section class="report-section"><h3>Findings (${findings.length})</h3>`;
  for (const f of findings) {
    const sev = esc(f.severity || 'note');
    html += `<div class="sdt-violation">`;
    html += `<div class="sdt-severity">${sev}${f.location ? ` · ${esc(f.location)}` : ''}</div>`;
    html += `<h4>${esc(f.title || 'Finding')}</h4>`;
    if (f.detail) html += `<p>${esc(f.detail)}</p>`;
    if (f.evidence) html += `<blockquote class="sdt-passage">${esc(f.evidence)}</blockquote>`;
    html += `</div>`;
  }
  html += `</section>`;
  return html;
}

function renderAiBetaReader(data: any): string {
  const chapters: any[] = data.chapters || [];
  let html = `<div class="zeigarnik-score"><span class="zeigarnik-pct">${esc(String(data.avg_engagement ?? '—'))}</span><span class="zeigarnik-label">Avg engagement</span></div>`;
  html += `<p class="report-hint">Avg put-down risk: ${esc(String(data.avg_put_down_risk ?? '—'))}</p>`;
  for (const ch of chapters) {
    html += `<section class="report-section"><h3>${esc(ch.title || ch.file || 'Chapter')}</h3>`;
    html += `<p><strong>Engagement ${esc(String(ch.engagement ?? '—'))}</strong> · Put-down risk ${esc(String(ch.put_down_risk ?? '—'))}</p>`;
    if (ch.reaction) html += `<p>${esc(ch.reaction)}</p>`;
    if (Array.isArray(ch.highlights) && ch.highlights.length) {
      html += `<p><strong>Highlights</strong></p><ul>${ch.highlights.map((h: string) => `<li>${esc(h)}</li>`).join('')}</ul>`;
    }
    if (Array.isArray(ch.friction) && ch.friction.length) {
      html += `<p><strong>Friction</strong></p><ul>${ch.friction.map((h: string) => `<li>${esc(h)}</li>`).join('')}</ul>`;
    }
    if (Array.isArray(ch.put_down_reasons) && ch.put_down_reasons.length) {
      html += `<p><strong>Put-down reasons</strong></p><ul>${ch.put_down_reasons.map((h: string) => `<li>${esc(h)}</li>`).join('')}</ul>`;
    }
    html += `</section>`;
  }
  return html;
}

function renderCliffhangerScore(data: any): string {
  const chapters: any[] = data.chapters || [];
  let html = `<div class="zeigarnik-score"><span class="zeigarnik-pct">${esc(String(data.avg_score ?? '—'))}</span><span class="zeigarnik-label">Avg ending pull</span></div>`;
  html += `<table class="report-table"><thead><tr><th>Chapter</th><th>Score</th><th>Type</th><th>Why</th></tr></thead><tbody>`;
  for (const ch of chapters) {
    html += `<tr><td>${esc(ch.title || ch.file || '')}</td><td>${esc(String(ch.score ?? ''))}</td><td>${esc(ch.ending_type || '')}</td><td>${esc(ch.why || '')}</td></tr>`;
  }
  html += `</tbody></table>`;
  return html;
}

function renderHookStrength(data: any): string {
  let html = `<div class="zeigarnik-score"><span class="zeigarnik-pct">${esc(String(data.score ?? '—'))}</span><span class="zeigarnik-label">${esc(data.verdict || 'Hook')}</span></div>`;
  if (data.summary) html += `<p>${esc(data.summary)}</p>`;
  if (Array.isArray(data.strengths) && data.strengths.length) {
    html += `<section class="report-section"><h3>Strengths</h3><ul>${data.strengths.map((s: string) => `<li>${esc(s)}</li>`).join('')}</ul></section>`;
  }
  if (Array.isArray(data.weaknesses) && data.weaknesses.length) {
    html += `<section class="report-section"><h3>Weaknesses</h3><ul>${data.weaknesses.map((s: string) => `<li>${esc(s)}</li>`).join('')}</ul></section>`;
  }
  if (data.first_friction_point) {
    html += `<p class="report-hint">First friction: ${esc(data.first_friction_point)}</p>`;
  }
  return html;
}

function renderPacingCurve(data: any): string {
  const chapters: any[] = data.chapters || [];
  let html = `<section class="report-section"><h3>Pacing by chapter</h3>`;
  html += `<table class="report-table"><thead><tr><th>Chapter</th><th>Pace</th><th>Drag risk</th><th>Label</th><th>Why</th></tr></thead><tbody>`;
  for (const ch of chapters) {
    html += `<tr><td>${esc(ch.title || ch.file || '')}</td><td>${esc(String(ch.pace_score ?? ''))}</td><td>${esc(String(ch.drag_risk ?? ''))}</td><td>${esc(ch.label || '')}</td><td>${esc(ch.why || '')}</td></tr>`;
  }
  html += `</tbody></table></section>`;
  return html;
}

function renderLinePolish(data: any): string {
  const totals = data.totals || {};
  let html = `<p class="report-hint">Heuristic scan — filter words ${esc(String(totals.filter_words ?? 0))}, echoes ${esc(String(totals.echoes ?? 0))}, adverbs ${esc(String(totals.adverbs ?? 0))}, passive ${esc(String(totals.passive ?? 0))}.</p>`;
  for (const ch of data.chapters || []) {
    const hits = ch.hits || {};
    const counts = ['filter_words', 'echoes', 'adverbs', 'passive']
      .map(k => `${k.replace('_', ' ')}: ${(hits[k] || []).length}`)
      .join(' · ');
    html += `<section class="report-section"><h3>${esc(ch.title || ch.file || '')}</h3><p class="report-hint">${esc(counts)}</p>`;
    for (const key of ['filter_words', 'echoes', 'adverbs', 'passive']) {
      const list = hits[key] || [];
      if (!list.length) continue;
      html += `<h4>${esc(key.replace(/_/g, ' '))}</h4><ul>`;
      for (const h of list.slice(0, 15)) {
        html += `<li><code>${esc(h.word || h.phrase || '')}</code> — ${esc(h.context || '')}</li>`;
      }
      if (list.length > 15) html += `<li class="muted">…and ${list.length - 15} more</li>`;
      html += `</ul>`;
    }
    html += `</section>`;
  }
  return html;
}

function renderBlurbBuilder(data: any): string {
  const variants: any[] = data.variants || [];
  let html = `<section class="report-section"><h3>Blurb Builder</h3>`;
  html += `<p class="report-hint">Three angles for Amazon, wide retailers, and back-cover copy.</p>`;
  for (const v of variants) {
    html += `<h4>${esc(v.angle || 'Variant')}</h4>`;
    html += `<p class="blurb-text">${esc(v.blurb || '')}</p>`;
    const c = v.critique || {};
    if (c.hook_strength) html += `<p><strong>Hook:</strong> ${esc(c.hook_strength)}</p>`;
    if (c.promise_vs_manuscript) html += `<p><strong>Promise vs manuscript:</strong> ${esc(c.promise_vs_manuscript)}</p>`;
  }
  if (data.short_form) {
    html += `<h4>Short form (~40 words)</h4><p>${esc(data.short_form)}</p>`;
  }
  if (data.bookbub_one_liner) {
    html += `<h4>BookBub one-liner</h4><p>${esc(data.bookbub_one_liner)}</p>`;
  }
  html += `</section>`;
  return html;
}

function renderPrintProduction(data: any): string {
  let html = `<section class="report-section"><h3>Print Production</h3>`;
  html += `<p><strong>Word count:</strong> ${(data.word_count || 0).toLocaleString()}</p>`;
  html += `<p><strong>Estimated pages:</strong> ${data.estimated_pages || '—'} (at ~250 words/page)</p>`;
  html += `<p><strong>Trim size:</strong> ${esc(data.trim_size || '')} — ${esc(data.trim_rationale || '')}</p>`;
  html += `<p><strong>Spine width:</strong> ${esc(data.spine_inches || '')} in</p>`;
  html += `<p><strong>Paper:</strong> ${esc(data.paper_color || '')} / <strong>Ink:</strong> ${esc(data.ink || 'black')}</p>`;
  const ingram: string[] = data.ingram_checklist || [];
  if (ingram.length) {
    html += `<h4>Ingram / wide print checklist</h4><ul>`;
    for (const item of ingram) html += `<li>${esc(item)}</li>`;
    html += `</ul>`;
  }
  const kdp: string[] = data.kdp_print_checklist || [];
  if (kdp.length) {
    html += `<h4>KDP Print checklist</h4><ul>`;
    for (const item of kdp) html += `<li>${esc(item)}</li>`;
    html += `</ul>`;
  }
  html += `</section>`;
  return html;
}

function renderVellumPrep(data: any): string {
  let html = `<section class="report-section"><h3>Clean manuscript ready</h3>`;
  html += `<p>${esc(String(data.chapter_count ?? 0))} chapters · ${esc(String(data.word_count ?? 0))} words</p>`;
  if (Array.isArray(data.notes)) {
    html += `<ul>${data.notes.map((n: string) => `<li>${esc(n)}</li>`).join('')}</ul>`;
  }
  if (data.clean_markdown) {
    const preview = String(data.clean_markdown).slice(0, 2000);
    html += `<h4>Manuscript preview</h4><pre class="report-raw">${esc(preview)}${data.clean_markdown.length > 2000 ? '\n\n[Truncated — full text stored in report JSON]' : ''}</pre>`;
  }
  html += `</section>`;
  return html;
}

function esc(s: string): string {
  if (!s) return '';
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}
