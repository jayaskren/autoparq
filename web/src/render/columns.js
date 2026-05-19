import { ConfidenceBadge } from '../components/ConfidenceBadge.js';
import { ImpactStars } from '../components/ImpactStars.js';
import { modeCodec, fullCodec } from '../lib/codec.js';

function humanBytes(n) {
  if (n >= 1_073_741_824) return (n / 1_073_741_824).toFixed(1) + ' GB';
  if (n >= 1_048_576)     return (n / 1_048_576).toFixed(1) + ' MB';
  if (n >= 1_024)         return (n / 1_024).toFixed(0) + ' KB';
  return n + ' B';
}

const DIAG_STATUS_META = {
  // ✓ green — already at recommended settings, no action needed
  Match:              { pill: '✓ match',     label: 'At recommended settings',
                        cls: 'bg-success-subtle text-success-fg border border-success-emphasis/40' },
  // ⛔ red — severe; writer actually fell back, data is suboptimally compressed
  FallbackDictionary: { pill: '⛔ fallback',  label: 'Dictionary fallback detected — retune recommended',
                        cls: 'bg-danger-subtle text-danger-fg border border-danger-fg/40 font-semibold' },
  // ⚠ amber — change recommended
  IneffectiveEncoding:{ pill: '⚠ weak',      label: 'Encoding applied but ineffective',
                        cls: 'bg-attention-subtle text-attention-fg border border-attention-emphasis/40' },
  Mismatch:           { pill: '⚠ change',    label: 'Change recommended',
                        cls: 'bg-attention-subtle text-attention-fg border border-attention-emphasis/40' },
};

function escapeText(s) {
  return String(s ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

const ENCODING_REFS = {
  DeltaMonotonic:  { label: 'Parquet DELTA_BINARY_PACKED spec', url: 'https://github.com/apache/parquet-format/blob/master/Encodings.md' },
  RleDictionary:   { label: 'Dictionary encoding &amp; the 1 MB fallback (Arrow blog)', url: 'https://arrow.apache.org/blog/2019/09/05/faster-strings-cpp-parquet/' },
  ByteStreamSplit: { label: 'Parquet BYTE_STREAM_SPLIT spec', url: 'https://github.com/apache/parquet-format/blob/master/Encodings.md' },
  PlainUuid:       { label: 'Dictionary overflow hazard (PARQUET-2052)', url: 'https://github.com/apache/parquet-java/pull/910' },
  BooleanRle:      { label: 'Parquet RLE spec', url: 'https://github.com/apache/parquet-format/blob/master/Encodings.md' },
};

/**
 * Renders the columns section with card/table toggle and filter bar.
 *
 * @param {HTMLElement} container
 * @param {object} report
 * @param {object|Function} [optsOrBenchFn] — either an options object
 *        `{ benchColumnFn, perColumnDiff }` or a bare `benchColumnFn` (legacy).
 */
export function renderColumns(container, report, optsOrBenchFn) {
  // Support both the new options-object form and the legacy bare function arg.
  let benchColumnFn, perColumnDiff;
  if (typeof optsOrBenchFn === 'function') {
    benchColumnFn = optsOrBenchFn;
    perColumnDiff = undefined;
  } else {
    benchColumnFn = optsOrBenchFn?.benchColumnFn;
    perColumnDiff = optsOrBenchFn?.perColumnDiff;
  }

  const deltaByName = {};
  if (Array.isArray(perColumnDiff)) {
    for (const d of perColumnDiff) {
      deltaByName[d.column_name] = d;
    }
  }
  const section = document.createElement('section');
  section.id = 'section-columns';

  // State
  let viewMode = 'cards'; // 'cards' | 'table' | 'chart'
  let nameFilter = '';
  let sortMode = 'impact'; // 'impact' | 'name' | 'confidence'
  let changedOnly = false;
  let tabulatorInstance = null;

  // F02: file-wide recommended codec — most per-card codec display is redundant
  const fileWideCodec = modeCodec(report.columns);

  section.innerHTML = `
    <div class="flex flex-wrap items-center justify-between gap-3 mb-4">
      <h2 class="text-lg font-semibold text-fg-default">Column Recommendations</h2>
      <div class="flex items-center gap-2">
        <button id="view-cards-btn" class="view-toggle-btn px-3 py-1 text-xs rounded-md transition-colors">Cards</button>
        <button id="view-table-btn" class="view-toggle-btn px-3 py-1 text-xs rounded-md transition-colors">Table</button>
        <button id="view-chart-btn" class="view-toggle-btn px-3 py-1 text-xs rounded-md transition-colors">Chart</button>
      </div>
    </div>

    <!-- Filter bar -->
    <div class="flex flex-wrap gap-3 mb-4 items-center">
      <input id="col-name-filter" type="text" placeholder="Filter columns..."
        class="bg-canvas-default border border-border-default text-fg-default text-sm rounded-md px-3 py-1.5 w-48 focus:outline-none focus:border-accent-emphasis placeholder-fg-muted" />
      <select id="col-sort-select"
        class="bg-canvas-default border border-border-default text-fg-default text-sm rounded-md px-3 py-1.5 focus:outline-none focus:border-accent-emphasis">
        <option value="impact">High impact first</option>
        <option value="name">Column name</option>
        <option value="confidence">Confidence</option>
      </select>
      <label class="flex items-center gap-2 text-sm text-fg-muted cursor-pointer select-none">
        <input id="changed-only-checkbox" type="checkbox" class="rounded border-border-default text-accent-emphasis" />
        Non-matching only
      </label>
    </div>

    <!-- Cards view -->
    <div id="cards-view"></div>

    <!-- Table view -->
    <div id="table-view" class="hidden"></div>

    <!-- Chart view -->
    <div id="chart-view" class="hidden"></div>
  `;

  container.appendChild(section);

  const cardsContainer = section.querySelector('#cards-view');
  const tableContainer = section.querySelector('#table-view');
  const chartContainer = section.querySelector('#chart-view');
  const cardBtn = section.querySelector('#view-cards-btn');
  const tableBtn = section.querySelector('#view-table-btn');
  const chartBtn = section.querySelector('#view-chart-btn');
  const nameFilterInput = section.querySelector('#col-name-filter');
  const sortSelect = section.querySelector('#col-sort-select');
  const changedOnlyCheck = section.querySelector('#changed-only-checkbox');

  // ── Lookup maps from file_profile ────────────────────────────────────────

  const currentEncodingMap = {};
  const currentCodecMap = {};
  const sizeMap = {};
  const diagnosticMap = {};
  for (const d of (report.diagnostics ?? [])) {
    diagnosticMap[d.column_name] = d;
  }

  let totalUncompressed = 0;

  if (report.file_profile?.columns) {
    totalUncompressed = report.file_profile.columns
      .reduce((s, c) => s + (c.uncompressed_bytes ?? 0), 0);

    for (const c of report.file_profile.columns) {
      const encs = c.encodings ?? [];
      const primary = encs.find(e => e !== 'PLAIN' && e !== 'RLE') ?? encs[0] ?? 'PLAIN';
      currentEncodingMap[c.name] = primary;
      currentCodecMap[c.name] = c.codec ?? report.current_codec;
      sizeMap[c.name] = {
        compressed: c.compressed_bytes ?? 0,
        share: totalUncompressed > 0 ? (c.uncompressed_bytes ?? 0) / totalUncompressed : 0,
      };
    }
  }

  // ── Per-column change helpers ─────────────────────────────────────────────

  function getRecCodecFull(col) {
    return fullCodec(col);
  }

  function columnChanges(col) {
    const currentEnc   = currentEncodingMap[col.column_name] ?? '—';
    const currentCodec = currentCodecMap[col.column_name] ?? report.current_codec ?? '—';
    const encChanged   = currentEnc !== '—' && currentEnc !== col.recommended_encoding;
    const codecChanged = currentCodec !== '—' && currentCodec !== getRecCodecFull(col);
    return { currentEnc, currentCodec, encChanged, codecChanged };
  }

  // Shared predicate — used by filter, sort secondary key, and card render.
  function isEffectivelyMatch(col) {
    if (diagnosticMap[col.column_name]?.status === 'Match') return true;
    const { encChanged, codecChanged } = columnChanges(col);
    if (encChanged) return false;
    if (!codecChanged) return true;
    const codecDiffersFromFileWide = fileWideCodec != null && getRecCodecFull(col) !== fileWideCodec;
    return !codecDiffersFromFileWide; // codec change is just the file-wide bump
  }

  // ── Filtering / sorting ───────────────────────────────────────────────────

  function getFilteredSortedColumns() {
    let cols = [...report.columns];

    if (nameFilter) {
      const lc = nameFilter.toLowerCase();
      cols = cols.filter(c => c.column_name.toLowerCase().includes(lc));
    }

    if (changedOnly) {
      cols = cols.filter(col => !isEffectivelyMatch(col));
    }

    switch (sortMode) {
      case 'impact':
        // Primary: impact_stars DESC. Secondary: non-match before match within the same tier,
        // so "needs change" columns float to the top even when they share an impact score.
        cols.sort((a, b) => {
          const dImpact = b.impact_stars - a.impact_stars;
          if (dImpact !== 0) return dImpact;
          const aMatch = isEffectivelyMatch(a) ? 1 : 0;
          const bMatch = isEffectivelyMatch(b) ? 1 : 0;
          if (aMatch !== bMatch) return aMatch - bMatch;
          return a.column_name.localeCompare(b.column_name);
        });
        break;
      case 'name':
        cols.sort((a, b) => a.column_name.localeCompare(b.column_name));
        break;
      case 'confidence': {
        const confOrder = { High: 0, Medium: 1, Low: 2 };
        cols.sort((a, b) => (confOrder[a.confidence] ?? 3) - (confOrder[b.confidence] ?? 3));
        break;
      }
    }

    return cols;
  }

  function renderCards() {
    const cols = getFilteredSortedColumns();
    cardsContainer.innerHTML = '';

    if (cols.length === 0) {
      cardsContainer.innerHTML = '<p class="text-fg-muted text-sm text-center py-8">No columns match your filter.</p>';
      return;
    }

    cols.forEach(col => cardsContainer.appendChild(buildColumnCard(col, benchColumnFn)));
  }

  // ── Card builder ──────────────────────────────────────────────────────────

  function buildColumnCard(col, onBench) {
    const { currentEnc, currentCodec, encChanged, codecChanged } = columnChanges(col);
    const recEnc      = col.recommended_encoding;
    const recCodecFull = getRecCodecFull(col);

    // F02: is this column's codec recommendation the same as the file-wide default?
    const codecDiffersFromFileWide = fileWideCodec != null && recCodecFull !== fileWideCodec;

    const diagnostic = diagnosticMap[col.column_name];

    // "Effectively match": the per-column action is nothing meaningful. Encoding is already correct
    // AND either the codec is already correct, or the codec change is a file-wide uniform bump
    // (which is surfaced in the Summary, not per-card). Treat these the same as a true Match.
    const encodingAlreadyCorrect = !encChanged;
    const codecChangeIsFileWide = codecChanged && !codecDiffersFromFileWide;
    const effectivelyMatch = (diagnostic?.status === 'Match')
      || (encodingAlreadyCorrect && (!codecChanged || codecChangeIsFileWide));

    // Display status — may override the Rust diagnostic for the "file-wide codec" case.
    const displayStatus = effectivelyMatch ? 'Match' : (diagnostic?.status ?? null);

    const card = document.createElement('div');
    // T18 — no left-border accent; F04 — muted class for Match
    card.className = `bg-canvas-subtle rounded-md border border-border-default mb-3 overflow-hidden ${effectivelyMatch ? 'card-muted' : ''}`;

    // Header row
    const header = document.createElement('div');
    header.className = 'flex flex-wrap items-center gap-3 p-4 cursor-pointer hover:bg-canvas-inset transition-colors';

    const nameSpan = document.createElement('span');
    nameSpan.className = 'font-mono font-semibold text-fg-default text-sm';
    nameSpan.textContent = col.column_name;

    const stars = ImpactStars(col.impact_stars);

    // F03 — confidence badge only when not High, repositioned to the right
    const needsConfidencePill = col.confidence && col.confidence !== 'High';
    const confPill = needsConfidencePill ? ConfidenceBadge(col.confidence) : null;

    // Diagnostic status pill — uses displayStatus so file-wide codec-only diffs don't look like mismatches
    let diagPill = null;
    if (displayStatus) {
      const meta = DIAG_STATUS_META[displayStatus];
      if (meta) {
        diagPill = document.createElement('span');
        diagPill.className = `font-mono text-xs rounded-full px-2 py-0.5 ${meta.cls}`;
        diagPill.textContent = meta.pill;
        diagPill.title = meta.label;
      }
    }

    const typeSpan = document.createElement('span');
    typeSpan.className = 'font-mono text-xs bg-neutral-subtle text-fg-muted rounded-full px-2 py-0.5';
    typeSpan.textContent = col.logical_type ?? col.physical_type;

    // Size badge (U01)
    const sz = sizeMap[col.column_name];
    const sizeSpan = document.createElement('span');
    if (sz) {
      const colorClass = sz.share >= 0.10 ? 'text-attention-fg font-semibold'
                       : sz.share >= 0.02 ? 'text-fg-default'
                       : 'text-fg-muted';
      sizeSpan.className = `font-mono text-xs ${colorClass} ml-auto`;
      sizeSpan.textContent = humanBytes(sz.compressed);
    } else {
      sizeSpan.className = 'ml-auto';
    }

    const chevron = document.createElement('span');
    chevron.className = 'text-fg-muted text-xs ml-1 transition-transform duration-200';
    chevron.textContent = '▼';

    // Left cluster: name + impact stars + diagnostic pill
    // Right cluster: (confidence pill if Medium/Low) + type + size + chevron
    const leftChildren = [nameSpan, stars];
    if (diagPill) leftChildren.push(diagPill);

    const rightChildren = [];
    if (confPill) rightChildren.push(confPill);
    rightChildren.push(typeSpan, sizeSpan, chevron);

    for (const el of leftChildren) header.appendChild(el);
    // spacer to push right cluster; sizeSpan already has ml-auto so leave as-is
    for (const el of rightChildren) header.appendChild(el);

    // Pill HTML helpers (Primer style)
    function pill(text, dim = false) {
      const cls = dim
        ? 'font-mono text-xs px-2 py-0.5 rounded-full bg-neutral-subtle text-fg-muted border border-border-default'
        : 'font-mono text-xs px-2 py-0.5 rounded-full bg-neutral-subtle text-fg-default border border-border-default';
      return `<span class="${cls}">${text}</span>`;
    }
    function pillHighlight(text) {
      return `<span class="font-mono text-xs px-2 py-0.5 rounded-full bg-accent-subtle text-accent-fg border border-accent-emphasis/30">${text}</span>`;
    }

    // Compact "At recommended settings" footer (F04) — replaces compareRow/reasonRow for match cards
    let compactFooter = null;
    let compareRow = null;
    let reasonRow = null;

    if (effectivelyMatch) {
      compactFooter = document.createElement('div');
      compactFooter.className = 'px-4 pb-3 text-xs text-fg-subtle';
      // If the codec bump is file-wide, be explicit about it so users don't wonder why this is "match"
      compactFooter.textContent = codecChangeIsFileWide
        ? `At recommended encoding. Codec will update to ${fileWideCodec} file-wide.`
        : 'At recommended settings.';
    } else {
      compareRow = document.createElement('div');
      compareRow.className = 'px-4 pb-3 flex items-stretch gap-3';

      // F02: omit codec pill row when codec matches file-wide default
      const showCodec = codecDiffersFromFileWide;

      const beforeHtml = `
        <div class="flex flex-col gap-1.5">
          <div class="text-xs text-fg-muted uppercase tracking-wide font-semibold mb-0.5">Current</div>
          ${pill(currentEnc, encChanged)}
          ${showCodec ? pill(currentCodec, codecChanged) : ''}
        </div>`;

      const arrowHtml = `<div class="flex items-center text-fg-muted text-lg font-light self-center px-1">→</div>`;

      const afterHtml = `
        <div class="flex flex-col gap-1.5">
          <div class="text-xs text-fg-muted uppercase tracking-wide font-semibold mb-0.5">Recommended</div>
          ${encChanged   ? pillHighlight(recEnc)      : pill(recEnc)}
          ${showCodec ? (codecChanged ? pillHighlight(recCodecFull) : pill(recCodecFull)) : ''}
        </div>`;

      compareRow.innerHTML = beforeHtml + arrowHtml + afterHtml;

      reasonRow = document.createElement('div');
      reasonRow.className = 'px-4 pb-3 text-xs text-fg-muted font-mono';
      const codecCaveat = codecDiffersFromFileWide
        ? `  ·  codec differs from file default — typically for pre-compressed data`
        : '';
      reasonRow.textContent = col.reason_brief
        + (col.null_fraction > 0 ? `  ·  null: ${(col.null_fraction * 100).toFixed(1)}%` : '')
        + codecCaveat;
    }

    // Accordion
    const accordion = document.createElement('div');
    accordion.className = 'accordion-content border-t border-border-muted';

    let accordionHtml = '';

    // ── Current vs Recommended diff ─────────────────────────────────────
    if (diagnostic) {
      const statusMeta = DIAG_STATUS_META[displayStatus];
      const haveEncs = (diagnostic.current_encodings ?? []).join(', ') || '(none)';
      const haveCodec = diagnostic.current_codec;
      const ratio = diagnostic.current_compression_ratio;
      const recEncL = col.recommended_encoding;
      const recCodecL = recCodecFull;
      // F02: include codec in "Have"/"Recommend" lines only when it differs from file-wide
      const haveCodecPart = codecDiffersFromFileWide ? ` + ${escapeText(haveCodec)}` : '';
      const recCodecPart  = codecDiffersFromFileWide ? ` + ${escapeText(recCodecL)}` : '';

      if (effectivelyMatch) {
        // When the Rust diagnostic says Mismatch but it's just the file-wide codec bump,
        // render a tailored "effectively match" message rather than the mismatch observation.
        const message = diagnostic.status === 'Match'
          ? diagnostic.observation
          : codecChangeIsFileWide
            ? `Encoding matches recommendation. The codec bump to ${fileWideCodec} is applied file-wide.`
            : diagnostic.observation;
        accordionHtml += `
          <div class="px-4 pt-4 pb-3">
            <p class="text-xs font-semibold text-fg-muted uppercase tracking-wide mb-2">Current vs Recommended</p>
            <p class="text-xs text-success-fg">✓ ${escapeText(message)}</p>
          </div>
        `;
      } else {
        const variance = diagnostic.per_row_group_summary
          ? `<div class="text-xs text-fg-muted mt-1 ml-4">└ ${escapeText(diagnostic.per_row_group_summary)}</div>`
          : '';
        const cause = diagnostic.cause_hypothesis
          ? `<div class="mt-2 text-xs text-fg-default">${escapeText(diagnostic.cause_hypothesis)}</div>`
          : '';
        const metric = diagnostic.supporting_metric
          ? `<div class="mt-1 text-xs text-fg-muted font-mono">${escapeText(diagnostic.supporting_metric)}</div>`
          : '';
        const statusPill = statusMeta
          ? `<span class="font-mono text-xs rounded-full px-2 py-0.5 ${statusMeta.cls}">${statusMeta.pill}</span>`
          : '';

        accordionHtml += `
          <div class="px-4 pt-4 pb-3">
            <div class="flex items-center gap-2 mb-3">
              <p class="text-xs font-semibold text-fg-muted uppercase tracking-wide">Current vs Recommended</p>
              ${statusPill}
            </div>
            <div class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
              <span class="text-fg-muted uppercase tracking-wide font-semibold">Have</span>
              <span class="font-mono text-fg-default">${escapeText(haveEncs)}${haveCodecPart} <span class="text-fg-muted">(ratio ${ratio.toFixed(2)}×)</span></span>
              ${variance ? `<span></span>${variance}` : ''}
              <span class="text-fg-muted uppercase tracking-wide font-semibold">Recommend</span>
              <span class="font-mono text-accent-fg">${escapeText(recEncL)}${recCodecPart}</span>
            </div>
            <div class="mt-3 text-xs text-fg-default">${escapeText(diagnostic.observation)}</div>
            ${cause}
            ${metric}
          </div>
        `;
      }
    }

    if (col.full_explain) {
      const fe = col.full_explain;

      accordionHtml += `
        <div class="px-4 pt-4 pb-3">
          <p class="text-xs font-semibold text-fg-muted uppercase tracking-wide mb-3">Why this encoding?</p>
          <div class="space-y-2 mb-4">
            ${fe.reasoning_chain.map(r => `
              <div class="flex items-start gap-2 text-xs">
                <span class="${r.fired ? 'text-success-fg' : r.evaluated ? 'text-danger-fg' : 'text-fg-muted'} font-mono mt-0.5">
                  ${r.fired ? '✓' : r.evaluated ? '✗' : '·'}
                </span>
                <div>
                  <span class="font-mono text-fg-default">${r.rule_name}</span>
                  <span class="text-fg-muted ml-2">${r.outcome}</span>
                </div>
              </div>
            `).join('')}
          </div>
          ${fe.teach_yourself ? (() => {
            const ref = ENCODING_REFS[col.encoding_rule_fired];
            const link = ref
              ? `<a href="${ref.url}" target="_blank" rel="noopener noreferrer"
                    class="block mt-2 text-accent-fg hover:underline underline-offset-2">
                   Further reading: ${ref.label} →
                 </a>`
              : '';
            return `
              <div class="bg-accent-subtle border border-accent-emphasis/30 rounded-md p-3 text-xs text-fg-default">
                <span class="font-semibold text-accent-fg">Learn: </span>${fe.teach_yourself}
                ${link}
              </div>`;
          })() : ''}
        </div>
      `;

      accordionHtml += `
        <div class="px-4 pb-3 border-t border-border-muted pt-3">
          <p class="text-xs font-semibold text-fg-muted uppercase tracking-wide mb-3">Raw Statistics</p>
          <div class="grid grid-cols-2 gap-x-4 gap-y-1">
            ${Object.entries(fe.raw_stats)
              .filter(([, v]) => v !== null && v !== undefined)
              .map(([k, v]) => `
                <span class="font-mono text-xs text-fg-muted">${k}</span>
                <span class="font-mono text-xs text-fg-default">${typeof v === 'number' ? v.toFixed(v % 1 === 0 ? 0 : 4) : String(v)}</span>
              `).join('')}
          </div>
        </div>
      `;

      if (fe.alternatives_considered?.length > 0) {
        accordionHtml += `
          <div class="px-4 pb-3 border-t border-border-muted pt-3">
            <p class="text-xs font-semibold text-fg-muted uppercase tracking-wide mb-3">Alternatives Considered</p>
            <div class="space-y-1">
              ${fe.alternatives_considered.map(a => `
                <div class="flex gap-3 text-xs">
                  <span class="font-mono text-fg-default w-40 shrink-0">${a.encoding}</span>
                  <span class="text-fg-muted">${a.rejected_reason}</span>
                </div>
              `).join('')}
            </div>
          </div>
        `;
      }

      if (fe.engine_compatibility) {
        accordionHtml += `
          <div class="px-4 pb-3 border-t border-border-muted pt-3">
            <p class="text-xs font-semibold text-fg-muted uppercase tracking-wide mb-2">Engine Compatibility</p>
            <p class="text-xs text-fg-default">${fe.engine_compatibility}</p>
          </div>
        `;
      }
    } else {
      accordionHtml = `
        <div class="px-4 py-3 text-xs text-fg-muted">
          Detailed explanation available. Run with <code class="font-mono">--explain full</code> for reasoning chain.
        </div>
      `;
    }

    accordionHtml += `
      <div class="px-4 pb-4 border-t border-border-muted pt-3">
        <p class="text-xs font-semibold text-fg-muted uppercase tracking-wide mb-1">Confidence</p>
        <p class="text-xs text-fg-default font-mono">${col.confidence_reason}</p>
      </div>
    `;

    accordion.innerHTML = accordionHtml;

    // Bench button
    if (onBench) {
      const benchSection = document.createElement('div');
      benchSection.className = 'px-4 pb-4 border-t border-border-muted pt-3';

      const benchBtn = document.createElement('button');
      benchBtn.className = 'text-xs px-3 py-1.5 rounded-md bg-canvas-default hover:bg-canvas-inset text-fg-default border border-border-default transition-colors';
      benchBtn.textContent = 'Benchmark this column';

      benchSection.appendChild(benchBtn);
      accordion.appendChild(benchSection);

      benchBtn.addEventListener('click', async () => {
        benchBtn.disabled = true;
        benchBtn.textContent = 'Benchmarking…';
        benchBtn.className = 'text-xs px-3 py-1.5 rounded-md bg-canvas-subtle text-fg-muted border border-border-default cursor-not-allowed';

        try {
          const result = await onBench(col.column_name);
          benchSection.innerHTML = '';
          benchSection.appendChild(renderBenchPanel(result, getRecCodecFull(col), col.recommended_encoding));
        } catch (err) {
          benchBtn.disabled = false;
          benchBtn.textContent = 'Benchmark this column';
          benchBtn.className = 'text-xs px-3 py-1.5 rounded-md bg-canvas-default hover:bg-canvas-inset text-fg-default border border-border-default transition-colors';
          const errMsg = document.createElement('p');
          errMsg.className = 'mt-2 text-xs text-danger-fg';
          errMsg.textContent = `Benchmark failed: ${err.message}`;
          benchSection.appendChild(errMsg);
        }
      });
    }

    let isOpen = false;
    header.addEventListener('click', () => {
      isOpen = !isOpen;
      accordion.classList.toggle('open', isOpen);
      chevron.style.transform = isOpen ? 'rotate(180deg)' : '';
    });

    card.appendChild(header);
    if (compareRow) card.appendChild(compareRow);
    if (reasonRow) card.appendChild(reasonRow);
    if (compactFooter) card.appendChild(compactFooter);

    // Spec 008 — measured before/after footer when a rewrite has been performed.
    const delta = deltaByName[col.column_name];
    if (delta) {
      const before = delta.before_compressed;
      const after = delta.after_compressed;
      const pct = before > 0 ? (1 - after / before) * 100 : 0;
      const cls = pct >= 5 ? 'text-success-fg'
                : pct <= -5 ? 'text-danger-fg'
                : 'text-fg-muted';
      const sign = pct >= 0 ? '−' : '+';
      const pctStr = `${sign}${Math.abs(pct).toFixed(1)}%`;

      const measured = document.createElement('div');
      measured.className = 'px-4 pb-3 text-xs font-mono border-t border-border-muted pt-2';
      measured.innerHTML = `
        <span class="text-fg-subtle uppercase tracking-wide">Measured </span>
        <span class="text-fg-default">${humanBytes(before)} → ${humanBytes(after)}</span>
        <span class="${cls} ml-1">(${pctStr})</span>
      `;
      card.appendChild(measured);
    }

    card.appendChild(accordion);
    return card;
  }

  // ── Bench results panel ───────────────────────────────────────────────────

  function renderBenchPanel(result, recCodecFull, recEncoding) {
    const panel = document.createElement('div');

    const header = document.createElement('div');
    header.className = 'flex items-center justify-between mb-3';
    header.innerHTML = `
      <p class="text-xs font-semibold text-fg-muted uppercase tracking-wide">
        Benchmark Results
        <span class="ml-2 font-mono text-fg-subtle normal-case">${result.physical_type}</span>
      </p>
    `;
    const closeBtn = document.createElement('button');
    closeBtn.className = 'text-xs text-fg-muted hover:text-fg-default';
    closeBtn.textContent = '✕ Close';
    header.appendChild(closeBtn);
    panel.appendChild(header);

    const table = document.createElement('table');
    table.className = 'w-full text-xs';
    table.innerHTML = `
      <thead>
        <tr class="text-fg-muted border-b border-border-muted">
          <th class="text-left font-normal pb-1.5 pr-3">Encoding</th>
          <th class="text-left font-normal pb-1.5 pr-3">Codec</th>
          <th class="text-right font-normal pb-1.5 pr-3">Compressed</th>
          <th class="text-right font-normal pb-1.5 pr-3">Ratio</th>
          <th class="text-right font-normal pb-1.5 pr-3">Write ms</th>
          <th class="text-right font-normal pb-1.5">Read ms</th>
        </tr>
      </thead>
    `;

    const tbody = document.createElement('tbody');
    for (const entry of result.entries) {
      const codecFull = entry.codec_level != null ? `${entry.codec}:${entry.codec_level}` : entry.codec;
      const isRec = entry.encoding === recEncoding && codecFull === recCodecFull;

      const tr = document.createElement('tr');
      tr.className = isRec
        ? 'bg-success-subtle border border-success-emphasis/30'
        : 'border-b border-border-muted';

      tr.innerHTML = `
        <td class="py-1 pr-3 font-mono ${isRec ? 'text-success-fg font-semibold' : 'text-fg-default'}">${entry.encoding}${isRec ? ' ★' : ''}</td>
        <td class="py-1 pr-3 font-mono text-fg-muted">${codecFull}</td>
        <td class="py-1 pr-3 font-mono text-right text-fg-default">${humanBytes(entry.compressed_bytes)}</td>
        <td class="py-1 pr-3 font-mono text-right text-fg-muted">${entry.compression_ratio.toFixed(2)}×</td>
        <td class="py-1 pr-3 font-mono text-right text-fg-muted">${entry.write_ms}</td>
        <td class="py-1 font-mono text-right text-fg-muted">${entry.read_ms}</td>
      `;
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);
    panel.appendChild(table);

    const footer = document.createElement('p');
    footer.className = 'mt-2 text-xs text-fg-muted';
    footer.textContent = 'Size measurements are exact. Timing resolution ≈ 1 ms (browser Spectre mitigation). Benchmarked on first row group, up to 500,000 rows.';
    panel.appendChild(footer);

    closeBtn.addEventListener('click', () => {
      panel.remove();
    });

    return panel;
  }

  // ── Chart view ────────────────────────────────────────────────────────────

  function renderChart() {
    const profileCols = report.file_profile?.columns ?? [];
    if (profileCols.length === 0) {
      chartContainer.innerHTML = '<p class="text-fg-muted text-sm text-center py-8">No column size data available.</p>';
      return;
    }

    const sorted = [...profileCols].sort((a, b) => (b.compressed_bytes ?? 0) - (a.compressed_bytes ?? 0));
    const maxBytes = sorted[0]?.compressed_bytes ?? 1;
    const totalUncompressedLocal = profileCols.reduce((s, c) => s + (c.uncompressed_bytes ?? 0), 0);

    const filterLc = nameFilter.toLowerCase();

    chartContainer.innerHTML = '';
    const wrap = document.createElement('div');
    wrap.className = 'space-y-1';

    const headerRow = document.createElement('div');
    headerRow.className = 'flex items-center gap-2 mb-2 px-1';
    headerRow.innerHTML = `
      <span class="text-xs text-fg-muted uppercase tracking-wide w-44 shrink-0">Column</span>
      <span class="text-xs text-fg-muted uppercase tracking-wide flex-1">Compressed size</span>
      <span class="text-xs text-fg-muted uppercase tracking-wide w-20 text-right shrink-0">Size</span>
      <span class="text-xs text-fg-muted uppercase tracking-wide w-20 text-right shrink-0">Ratio</span>
    `;
    wrap.appendChild(headerRow);

    // GitHub-inspired bar colors
    const COLOR_HIGH = '#bf8700';    // attention emphasis
    const COLOR_MID  = '#0969da';    // accent emphasis
    const COLOR_LOW  = '#afb8c1';    // neutral / muted

    for (const col of sorted) {
      const compressed = col.compressed_bytes ?? 0;
      const uncompressed = col.uncompressed_bytes ?? 0;
      const share = totalUncompressedLocal > 0 ? (uncompressed / totalUncompressedLocal) : 0;
      const barPct = maxBytes > 0 ? (compressed / maxBytes) * 100 : 0;
      const ratio = compressed > 0 ? (uncompressed / compressed).toFixed(2) : '—';

      const barColor = share >= 0.10 ? COLOR_HIGH
                     : share >= 0.02 ? COLOR_MID
                     : COLOR_LOW;
      const textClass  = share >= 0.10 ? 'text-attention-fg font-semibold'
                       : share >= 0.02 ? 'text-fg-default'
                       : 'text-fg-muted';

      const dimmed = filterLc && !col.name.toLowerCase().includes(filterLc);

      const { encChanged, codecChanged } = columnChanges({ column_name: col.name, recommended_encoding: '', recommended_codec: '', recommended_codec_level: null });
      const dotColor = encChanged ? COLOR_HIGH : codecChanged ? COLOR_MID : COLOR_LOW;

      const row = document.createElement('div');
      row.className = `flex items-center gap-2 px-1 py-0.5 rounded hover:bg-canvas-subtle transition-colors ${dimmed ? 'opacity-30' : ''}`;
      row.innerHTML = `
        <div class="flex items-center gap-1.5 w-44 shrink-0 min-w-0">
          <span class="w-2 h-2 rounded-full shrink-0" style="background:${dotColor}"></span>
          <span class="font-mono text-xs ${textClass} truncate" title="${col.name}">${col.name}</span>
        </div>
        <div class="flex-1 bg-canvas-inset rounded-full h-3 overflow-hidden">
          <div class="h-full rounded-full transition-all" style="width:${barPct.toFixed(1)}%;background:${barColor}"></div>
        </div>
        <span class="font-mono text-xs ${textClass} w-20 text-right shrink-0">${humanBytes(compressed)}</span>
        <span class="font-mono text-xs text-fg-muted w-20 text-right shrink-0">${ratio}×</span>
      `;
      wrap.appendChild(row);
    }

    const legend = document.createElement('div');
    legend.className = 'flex flex-wrap gap-4 mt-4 pt-3 border-t border-border-muted text-xs text-fg-muted';
    legend.innerHTML = `
      <span><span class="inline-block w-2 h-2 rounded-full mr-1" style="background:${COLOR_HIGH}"></span>amber dot = encoding changes</span>
      <span><span class="inline-block w-2 h-2 rounded-full mr-1" style="background:${COLOR_MID}"></span>blue = codec-only change</span>
      <span><span class="inline-block w-2 h-2 rounded-full mr-1" style="background:${COLOR_LOW}"></span>gray = no change needed</span>
      <span class="ml-auto">Bar width = relative compressed size. Ratio = uncompressed ÷ compressed.</span>
    `;
    wrap.appendChild(legend);

    chartContainer.appendChild(wrap);
  }

  // ── Table view ────────────────────────────────────────────────────────────

  function initTable() {
    import('tabulator-tables').then(({ TabulatorFull }) => {
      const cols = getFilteredSortedColumns();
      const confidenceOrder = { High: 1, Medium: 2, Low: 3 };

      tabulatorInstance = new TabulatorFull(tableContainer, {
        data: cols,
        layout: 'fitDataFill',
        height: '520px',
        columns: [
          {
            title: 'Column',
            field: 'column_name',
            frozen: true,
            headerFilter: false,
            formatter: (cell) => `<span class="font-mono text-sm" style="color:#1f2328">${cell.getValue()}</span>`,
            minWidth: 160,
            widthGrow: 2,
          },
          {
            title: 'Size',
            field: 'column_name',
            formatter: (cell) => {
              const sz = sizeMap[cell.getValue()];
              if (!sz) return '—';
              const color = sz.share >= 0.10 ? '#9a6700'
                          : sz.share >= 0.02 ? '#1f2328'
                          : '#656d76';
              return `<span class="font-mono text-xs" style="color:${color}">${humanBytes(sz.compressed)}</span>`;
            },
            width: 90,
            headerSort: false,
          },
          {
            title: 'Type',
            field: 'physical_type',
            formatter: (cell) => `<span class="font-mono text-xs" style="color:#656d76">${cell.getValue()}</span>`,
            width: 90,
          },
          {
            title: 'Null %',
            field: 'null_fraction',
            formatter: (cell) => `<span class="font-mono text-xs">${(cell.getValue() * 100).toFixed(1)}%</span>`,
            width: 75,
            sorter: 'number',
          },
          {
            title: 'Cardinality',
            field: 'cardinality_ratio',
            formatter: (cell) => `<span class="font-mono text-xs">${(cell.getValue() * 100).toFixed(2)}%</span>`,
            width: 100,
            sorter: 'number',
          },
          {
            title: 'Current encoding',
            field: 'column_name',
            formatter: (cell) => {
              const cur = currentEncodingMap[cell.getValue()] ?? '—';
              return `<span class="font-mono text-xs" style="color:#656d76">${cur}</span>`;
            },
            width: 170,
            headerSort: false,
          },
          {
            title: 'Recommended encoding',
            field: 'recommended_encoding',
            formatter: (cell) => {
              const row = cell.getRow().getData();
              const cur = currentEncodingMap[row.column_name] ?? '—';
              const rec = cell.getValue();
              const changed = cur !== '—' && cur !== rec;
              return `<span class="font-mono text-xs" style="color:${changed ? '#0969da' : '#656d76'};${changed ? 'font-weight:600' : ''}">${rec}</span>`;
            },
            width: 190,
          },
          {
            title: 'Current codec',
            field: 'column_name',
            formatter: (cell) => {
              const cur = currentCodecMap[cell.getValue()] ?? report.current_codec ?? '—';
              return `<span class="font-mono text-xs" style="color:#656d76">${cur}</span>`;
            },
            width: 120,
            headerSort: false,
          },
          {
            title: 'Recommended codec',
            field: 'recommended_codec',
            formatter: (cell) => {
              const row = cell.getRow().getData();
              const cur = currentCodecMap[row.column_name] ?? report.current_codec ?? '—';
              const rec = row.recommended_codec_level != null
                ? `${cell.getValue()}:${row.recommended_codec_level}`
                : cell.getValue();
              const changed = cur !== '—' && cur !== rec;
              return `<span class="font-mono text-xs" style="color:${changed ? '#0969da' : '#656d76'};${changed ? 'font-weight:600' : ''}">${rec}</span>`;
            },
            width: 155,
          },
          {
            title: 'Confidence',
            field: 'confidence',
            formatter: (cell) => {
              const tier = cell.getValue();
              const color = { High: '#1a7f37', Medium: '#9a6700', Low: '#d1242f' }[tier] ?? '#656d76';
              return `<span class="font-mono text-xs" style="color:${color};font-weight:600">${tier?.toUpperCase()}</span>`;
            },
            sorter: (a, b) => (confidenceOrder[a] ?? 4) - (confidenceOrder[b] ?? 4),
            width: 105,
          },
          {
            title: 'Impact',
            field: 'impact_stars',
            formatter: (cell) => {
              const n = cell.getValue();
              return Array.from({length: 5}, (_, i) =>
                `<span style="color:${i < n ? '#bf8700' : '#afb8c1'}">★</span>`
              ).join('');
            },
            sorter: 'number',
            width: 90,
          },
        ],
        initialSort: [{ column: 'impact_stars', dir: 'desc' }],
      });
    });
  }

  function applyTableFilters() {
    if (!tabulatorInstance) return;
    const filters = [];
    if (nameFilter) filters.push({ field: 'column_name', type: 'like', value: nameFilter });
    tabulatorInstance.setFilter(filters);
  }

  const btnActive   = 'view-toggle-btn px-3 py-1 text-xs rounded-md bg-accent-emphasis text-fg-on-emphasis';
  const btnInactive = 'view-toggle-btn px-3 py-1 text-xs rounded-md bg-canvas-subtle text-fg-muted border border-border-default hover:bg-canvas-inset transition-colors';

  function updateViewToggle() {
    cardBtn.className  = viewMode === 'cards'  ? btnActive : btnInactive;
    tableBtn.className = viewMode === 'table'  ? btnActive : btnInactive;
    chartBtn.className = viewMode === 'chart'  ? btnActive : btnInactive;

    cardsContainer.classList.toggle('hidden', viewMode !== 'cards');
    tableContainer.classList.toggle('hidden', viewMode !== 'table');
    chartContainer.classList.toggle('hidden', viewMode !== 'chart');

    if (viewMode === 'table') {
      if (!tabulatorInstance) initTable();
      else applyTableFilters();
    } else if (viewMode === 'chart') {
      renderChart();
    }
  }

  // Event listeners
  nameFilterInput.addEventListener('input', (e) => {
    nameFilter = e.target.value.trim();
    if (viewMode === 'cards') renderCards();
    else if (viewMode === 'chart') renderChart();
    else applyTableFilters();
  });

  sortSelect.addEventListener('change', (e) => {
    sortMode = e.target.value;
    if (viewMode === 'cards') renderCards();
  });

  changedOnlyCheck.addEventListener('change', (e) => {
    changedOnly = e.target.checked;
    tabulatorInstance = null;
    if (viewMode === 'cards') renderCards();
    else initTable();
  });

  // Summary "File health" click dispatches this event
  document.addEventListener('autoparq:filter-non-matching', () => {
    if (!changedOnly) {
      changedOnly = true;
      changedOnlyCheck.checked = true;
      tabulatorInstance = null;
      if (viewMode === 'cards') renderCards();
      else if (viewMode === 'chart') renderChart();
      else initTable();
    }
  });

  cardBtn.addEventListener('click', () => {
    viewMode = 'cards';
    updateViewToggle();
    renderCards();
  });

  tableBtn.addEventListener('click', () => {
    viewMode = 'table';
    updateViewToggle();
  });

  chartBtn.addEventListener('click', () => {
    viewMode = 'chart';
    updateViewToggle();
  });

  updateViewToggle();
  renderCards();
}
