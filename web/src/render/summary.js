import { modeCodec, fullCodec } from '../lib/codec.js';
import { triggerDownload, tunedFilename } from '../lib/download.js';

function formatBytesInline(bytes) {
  if (bytes >= 1e9) return (bytes / 1e9).toFixed(1) + ' GB';
  if (bytes >= 1e6) return (bytes / 1e6).toFixed(1) + ' MB';
  if (bytes >= 1e3) return (bytes / 1e3).toFixed(1) + ' KB';
  return bytes + ' B';
}

/**
 * Renders the summary section with estimated improvement stats and encoding breakdown.
 *
 * @param {HTMLElement} container
 * @param {object} report
 * @param {object} [opts]
 * @param {Function} [opts.onApply]         — async (engine, priority) => { outputBytes, rewrite, perColumnDiff }
 * @param {Function} [opts.getRewriteState] — () => rewrite state or null
 * @param {Function} [opts.onDiscard]       — clears rewrite state, re-renders
 * @param {string}   [opts.fileName]        — used to suggest the download filename
 */
export function renderSummary(container, report, opts = {}) {
  const { onApply, getRewriteState, onDiscard, fileName } = opts;
  const rewriteState = getRewriteState?.() ?? null;

  const section = document.createElement('section');
  section.id = 'section-summary';

  // Compute encoding breakdown
  const encodingCounts = {};
  for (const col of report.columns) {
    const enc = col.recommended_encoding;
    encodingCounts[enc] = (encodingCounts[enc] || 0) + 1;
  }

  const encodingLines = Object.entries(encodingCounts)
    .sort((a, b) => b[1] - a[1])
    .map(([enc, count]) => `<div class="flex justify-between gap-4">
        <span class="font-mono text-xs text-fg-default">${enc}</span>
        <span class="text-xs text-fg-muted">${count} col${count > 1 ? 's' : ''}</span>
      </div>`)
    .join('');

  const currentSizeStr = formatBytes(report.file_size_bytes);
  const pct = report.predicted_size_reduction_pct;
  const pctLow = Math.floor(pct * 0.5);
  const pctHigh = Math.ceil(pct * 1.5);
  const estimatedBytesLow = report.file_size_bytes * (1 - pctHigh / 100);
  const estimatedBytesHigh = report.file_size_bytes * (1 - pctLow / 100);
  const estimatedRangeStr = `${formatBytes(estimatedBytesLow)} – ${formatBytes(estimatedBytesHigh)}`;

  const confidenceColor = {
    High: 'text-success-fg',
    Medium: 'text-attention-fg',
    Low: 'text-danger-fg',
  }[report.overall_confidence] ?? 'text-fg-muted';

  // File health breakdown (Spec 006, updated Spec 007)
  // A column that already has the recommended encoding AND the only codec diff is a file-wide
  // bump is considered "effectively match" — it needs no per-column action.
  const diagnostics = report.diagnostics ?? [];
  const fileWideCodec = modeCodec(report.columns);

  const fileProfileByName = {};
  for (const c of (report.file_profile?.columns ?? [])) fileProfileByName[c.name] = c;

  function isEffectivelyMatch(colRec, diag) {
    if (diag?.status === 'Match') return true;
    if (diag?.status === 'FallbackDictionary' || diag?.status === 'IneffectiveEncoding') return false;
    // For Mismatch: check if the mismatch is purely a file-wide codec bump
    const meta = fileProfileByName[colRec.column_name];
    if (!meta) return false;
    const currentEncs = meta.encodings ?? [];
    const primaryEnc = currentEncs.find(e => e !== 'PLAIN' && e !== 'RLE') ?? currentEncs[0] ?? 'PLAIN';
    const encMatches = primaryEnc === colRec.recommended_encoding;
    if (!encMatches) return false;
    const recCodec = fullCodec(colRec);
    const currentCodec = meta.codec ?? '';
    const codecChanged = currentCodec !== recCodec;
    const codecDiffersFromFileWide = fileWideCodec != null && recCodec !== fileWideCodec;
    return !codecChanged || !codecDiffersFromFileWide;
  }

  const diagByName = {};
  for (const d of diagnostics) diagByName[d.column_name] = d;

  const diagTotal      = report.columns.length;
  let diagMatched = 0, diagFallbacks = 0, diagWeak = 0, diagMismatches = 0;
  for (const col of report.columns) {
    const d = diagByName[col.column_name];
    if (isEffectivelyMatch(col, d)) { diagMatched++; continue; }
    const s = d?.status;
    if (s === 'FallbackDictionary') diagFallbacks++;
    else if (s === 'IneffectiveEncoding') diagWeak++;
    else diagMismatches++;
  }

  const healthDetailParts = [];
  if (diagFallbacks > 0) healthDetailParts.push(`${diagFallbacks} fallback${diagFallbacks > 1 ? 's' : ''}`);
  if (diagWeak > 0)      healthDetailParts.push(`${diagWeak} weak`);
  if (diagMismatches > 0) healthDetailParts.push(`${diagMismatches} mismatch${diagMismatches > 1 ? 'es' : ''}`);

  const healthHtml = diagTotal > 0 ? `
    <div class="flex justify-between items-start border-t border-border-muted pt-3">
      <span class="text-sm text-fg-muted">File health</span>
      <div class="flex flex-col items-end">
        <button id="file-health-link"
                class="font-mono text-sm text-fg-default hover:text-accent-fg transition-colors text-right"
                title="Click to show only non-matching columns">
          ${diagMatched} of ${diagTotal} match
        </button>
        ${healthDetailParts.length > 0
          ? `<span class="font-mono text-xs text-fg-muted mt-0.5">${healthDetailParts.join(', ')}</span>`
          : ''}
      </div>
    </div>
  ` : '';

  // F02a — file-wide recommended codec (already computed above, fallback for empty reports)
  const fileWideCodecDisplay = fileWideCodec ?? 'ZSTD:3';

  // Spec 008 — when a rewrite has been performed, swap estimated → measured.
  const isApplied = rewriteState != null;
  const measuredAfterBytes = isApplied ? rewriteState.rewrite.output_size_bytes : null;
  const measuredReductionPct = isApplied ? rewriteState.rewrite.actual_reduction_pct : null;

  const sizeAfterRowHtml = isApplied
    ? `<div class="flex justify-between items-baseline">
         <span class="text-sm text-fg-muted">Actual size after</span>
         <span class="font-mono text-sm text-success-fg">${formatBytesInline(measuredAfterBytes)} <span class="text-fg-muted text-xs font-normal">[measured]</span></span>
       </div>`
    : `<div class="flex justify-between items-baseline">
         <span class="text-sm text-fg-muted">Estimated size after</span>
         <span class="font-mono text-sm text-accent-fg">${estimatedRangeStr} <span class="text-fg-muted text-xs">[estimated range]</span></span>
       </div>`;

  const reductionRowHtml = isApplied
    ? `<div class="flex justify-between items-baseline border-t border-border-muted pt-3">
         <span class="text-sm text-fg-muted">Size reduction</span>
         <span class="font-mono text-sm font-semibold text-success-fg">−${measuredReductionPct.toFixed(1)}% <span class="text-fg-muted text-xs font-normal">[measured]</span></span>
       </div>`
    : `<div class="flex justify-between items-baseline border-t border-border-muted pt-3">
         <span class="text-sm text-fg-muted">Size reduction</span>
         <span class="font-mono text-sm font-semibold text-success-fg">−${pctLow}% – −${pctHigh}% <span class="text-fg-muted text-xs font-normal">[estimated range]</span></span>
       </div>`;

  section.innerHTML = `
    <h2 class="text-lg font-semibold text-fg-default mb-4">Summary</h2>
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">

      <!-- Left: Size + speed stats -->
      <div class="bg-canvas-subtle rounded-md border border-border-default p-5">
        <h3 class="text-xs font-semibold text-fg-muted uppercase tracking-wide mb-4">Size &amp; Speed Estimates</h3>
        <div class="space-y-3">
          <div class="flex justify-between items-baseline">
            <span class="text-sm text-fg-muted">Current size</span>
            <span class="font-mono text-sm text-fg-default">${currentSizeStr}</span>
          </div>
          ${sizeAfterRowHtml}
          ${reductionRowHtml}
          <div class="flex justify-between items-baseline border-t border-border-muted pt-3">
            <span class="text-sm text-fg-muted">Analysis confidence</span>
            <span class="font-mono text-sm font-semibold ${confidenceColor}">${report.overall_confidence}</span>
          </div>
          ${healthHtml}
        </div>
      </div>

      <!-- Right: Codec + encoding breakdown -->
      <div class="bg-canvas-subtle rounded-md border border-border-default p-5">
        <h3 class="text-xs font-semibold text-fg-muted uppercase tracking-wide mb-4">Recommended Settings</h3>
        <div class="space-y-3">
          <div class="flex justify-between items-baseline">
            <span class="text-sm text-fg-muted">Current codec</span>
            <span class="font-mono text-xs bg-neutral-subtle text-fg-default rounded-full px-2 py-0.5">${report.current_codec}</span>
          </div>
          <div class="flex justify-between items-baseline">
            <span class="text-sm text-fg-muted">Recommended codec</span>
            <span class="font-mono text-xs bg-accent-subtle text-accent-fg border border-accent-emphasis/30 rounded-full px-2 py-0.5">${fileWideCodecDisplay}</span>
          </div>
          <p class="text-xs text-fg-subtle -mt-1">Applied file-wide. Columns flagged below use a different codec.</p>
          <div class="border-t border-border-muted pt-3">
            <p class="text-xs text-fg-muted uppercase tracking-wide mb-2">Encoding breakdown</p>
            <div class="space-y-1">
              ${encodingLines}
            </div>
          </div>
        </div>
      </div>

    </div>

    <div id="apply-cta-slot" class="mt-4"></div>

    <p class="mt-3 text-xs text-fg-muted">
      Size estimates use heuristic compression factors and may vary 2× in either direction. Expand a column below and click <strong class="text-fg-default">Benchmark this column</strong> for exact measured sizes.
    </p>

    <p class="mt-2 text-xs text-fg-muted">
      Recommendations are based on measured column statistics. See
      <a href="https://github.com/your-org/autoparq/blob/main/docs/REFERENCES.md"
         target="_blank" rel="noopener noreferrer"
         class="text-accent-fg hover:underline underline-offset-2">reference sources</a>
      for the research and benchmarks behind each claim.
    </p>

  `;

  container.appendChild(section);

  // T14 — wire File health click to filter
  const healthBtn = section.querySelector('#file-health-link');
  if (healthBtn) {
    healthBtn.addEventListener('click', () => {
      document.dispatchEvent(new CustomEvent('autoparq:filter-non-matching'));
      document.getElementById('section-columns')
        ?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    });
  }

  // Spec 008 — Apply CTA / success strip
  const ctaSlot = section.querySelector('#apply-cta-slot');
  if (ctaSlot && onApply) {
    renderApplyBlock(ctaSlot, {
      report,
      rewriteState,
      fileName,
      onApply,
      onDiscard,
    });
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Apply CTA / measured-overlay success strip
// ─────────────────────────────────────────────────────────────────────────────

function renderApplyBlock(slot, ctx) {
  if (ctx.rewriteState) {
    renderSuccessStrip(slot, ctx);
  } else {
    renderApplyCta(slot, ctx);
  }
}

function renderApplyCta(slot, ctx) {
  const { report, onApply, fileName, onDiscard } = ctx;
  slot.innerHTML = `
    <div class="bg-accent-subtle border border-accent-emphasis/30 rounded-md p-5">
      <div class="flex flex-wrap items-center justify-between gap-4">
        <div class="flex-1 min-w-[200px]">
          <h3 class="font-semibold text-fg-default">Apply these recommendations</h3>
          <p class="text-sm text-fg-muted mt-1">
            Download a new copy of this file with the recommended encodings and codec.
            Runs locally in your browser — nothing is uploaded.
          </p>
          <div id="apply-size-notice" class="text-xs mt-2"></div>
        </div>
        <button id="apply-btn"
                class="bg-accent-emphasis text-fg-on-emphasis rounded-md px-4 py-2 font-medium hover:opacity-90 transition-opacity shrink-0">
          Download optimized copy
        </button>
      </div>
      <div id="apply-error" class="mt-3 text-xs hidden"></div>
    </div>
  `;

  const btn = slot.querySelector('#apply-btn');
  const notice = slot.querySelector('#apply-size-notice');
  const errBox = slot.querySelector('#apply-error');

  // Pre-flight size check for warnings/blocking.
  import('../wasm-bridge.js').then(({ checkApplySize }) => {
    checkApplySize(report.file_size_bytes).then(check => {
      if (check.severity === 'blocked') {
        btn.disabled = true;
        btn.className = 'bg-neutral-subtle text-fg-muted border border-border-default rounded-md px-4 py-2 font-medium cursor-not-allowed shrink-0';
        notice.innerHTML = `<span class="text-danger-fg">⛔ ${check.error}</span>`;
      } else if (check.severity === 'severe') {
        notice.innerHTML = `<span class="text-danger-fg">⚠ ${check.warning}</span>`;
      } else if (check.severity === 'mild') {
        notice.innerHTML = `<span class="text-attention-fg">⚠ ${check.warning}</span>`;
      }
    }).catch(() => {});
  });

  btn.addEventListener('click', async () => {
    if (btn.disabled) return;
    errBox.classList.add('hidden');

    // For severe tier, show an inline confirm step before proceeding (A02).
    const { checkApplySize } = await import('../wasm-bridge.js');
    const check = await checkApplySize(report.file_size_bytes);
    if (check.severity === 'blocked') return;

    if (check.severity === 'severe' && !btn.dataset.confirmed) {
      renderSevereConfirm(slot, ctx);
      return;
    }

    await runApply(slot, ctx);
  });
}

function renderSevereConfirm(slot, ctx) {
  const sizeStr = formatBytesInline(ctx.report.file_size_bytes);
  slot.innerHTML = `
    <div class="bg-danger-subtle border border-danger-fg/30 rounded-md p-5">
      <div class="flex items-start gap-3">
        <span class="text-danger-fg text-xl mt-0.5">⚠</span>
        <div class="flex-1">
          <h3 class="font-semibold text-fg-default">Large file (${sizeStr})</h3>
          <p class="text-sm text-fg-default mt-1">
            Rewriting this size may fail on memory-constrained browsers. If it fails, use the CLI snippet below instead.
          </p>
          <div class="mt-3 flex flex-wrap gap-2">
            <button id="apply-proceed" class="bg-accent-emphasis text-fg-on-emphasis rounded-md px-4 py-2 font-medium hover:opacity-90">
              Proceed anyway
            </button>
            <button id="apply-cancel" class="bg-canvas-default text-fg-default border border-border-default rounded-md px-4 py-2 font-medium hover:bg-canvas-inset">
              Cancel
            </button>
          </div>
        </div>
      </div>
    </div>
  `;
  slot.querySelector('#apply-proceed').addEventListener('click', async () => {
    await runApply(slot, ctx);
  });
  slot.querySelector('#apply-cancel').addEventListener('click', () => {
    renderApplyCta(slot, ctx);
  });
}

async function runApply(slot, ctx) {
  const { report, onApply, fileName } = ctx;

  slot.innerHTML = `
    <div class="bg-canvas-subtle border border-border-default rounded-md p-5 flex items-center gap-3">
      <div class="w-5 h-5 border-2 border-accent-emphasis border-t-transparent rounded-full animate-spin"></div>
      <div class="flex-1">
        <p class="text-sm text-fg-default font-medium">Rewriting ${formatBytesInline(report.file_size_bytes)} file…</p>
        <p class="text-xs text-fg-muted mt-0.5">May take up to 30 seconds. Runs locally — nothing is uploaded.</p>
      </div>
    </div>
  `;

  // Select the engine and priority as they are right now in the header.
  const engine = document.getElementById('engine-select')?.value
    ?? document.getElementById('header-engine-select')?.value
    ?? 'unknown';
  const priority = document.getElementById('priority-select')?.value ?? 'balanced';

  try {
    const result = await onApply(engine, priority);
    // Trigger the browser download.
    triggerDownload(result.outputBytes, tunedFilename(fileName ?? 'output.parquet'));
    // Re-render will be triggered by App.js after it stores state.
  } catch (err) {
    renderApplyError(slot, ctx, err);
  }
}

function renderApplyError(slot, ctx, err) {
  slot.innerHTML = `
    <div class="bg-danger-subtle border border-danger-fg/30 rounded-md p-5">
      <p class="font-semibold text-danger-fg">Rewrite failed</p>
      <p class="text-sm text-fg-default mt-1 font-mono">${err?.message ?? String(err)}</p>
      <div class="mt-3 flex gap-2">
        <button id="apply-retry" class="bg-canvas-default text-fg-default border border-border-default rounded-md px-3 py-1.5 text-sm hover:bg-canvas-inset">
          Try again
        </button>
        <a href="#snippet-panel" class="bg-canvas-default text-fg-default border border-border-default rounded-md px-3 py-1.5 text-sm hover:bg-canvas-inset no-underline"
           onclick="event.preventDefault(); document.getElementById('snippet-panel')?.scrollIntoView({behavior:'smooth', block:'start'})">
          Copy CLI snippet instead →
        </a>
      </div>
    </div>
  `;
  slot.querySelector('#apply-retry').addEventListener('click', () => {
    renderApplyCta(slot, ctx);
  });
}

function renderSuccessStrip(slot, ctx) {
  const { rewriteState, fileName, onDiscard } = ctx;
  const rw = rewriteState.rewrite;
  const inMB = formatBytesInline(rw.input_size_bytes);
  const outMB = formatBytesInline(rw.output_size_bytes);
  const pct = rw.actual_reduction_pct.toFixed(1);
  const canDownload = rewriteState.outputBytes != null;

  const appliedWith = rewriteState.engine && rewriteState.priority
    ? `${rewriteState.engine} · ${rewriteState.priority}`
    : '';

  slot.innerHTML = `
    <div class="bg-success-subtle border border-success-emphasis/40 rounded-md p-5">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div class="flex-1 min-w-[200px]">
          <p class="font-semibold text-success-fg">
            ✓ Applied. ${inMB} → ${outMB} <span class="font-mono">(−${pct}%)</span>
          </p>
          <p class="text-xs text-fg-muted mt-1">
            ${appliedWith ? `${appliedWith} · ` : ''}${canDownload ? 'downloaded copy expires soon' : 'downloaded copy already discarded'}
          </p>
        </div>
        <div class="flex gap-2 shrink-0">
          ${canDownload
            ? `<button id="apply-redownload" class="bg-accent-emphasis text-fg-on-emphasis rounded-md px-3 py-1.5 text-sm font-medium hover:opacity-90">Download again</button>`
            : `<button id="apply-reapply" class="bg-accent-emphasis text-fg-on-emphasis rounded-md px-3 py-1.5 text-sm font-medium hover:opacity-90">Re-apply to download</button>`}
        </div>
      </div>
    </div>
  `;

  if (canDownload) {
    slot.querySelector('#apply-redownload').addEventListener('click', () => {
      triggerDownload(rewriteState.outputBytes, tunedFilename(fileName ?? 'output.parquet'));
      // Reset the discard timer — App owns the timer, signal via event.
      document.dispatchEvent(new CustomEvent('autoparq:reset-discard-timer'));
    });
  } else {
    slot.querySelector('#apply-reapply').addEventListener('click', () => {
      // Clear the stale overlay first so Apply CTA renders fresh.
      onDiscard?.();
    });
  }
}

function formatBytes(bytes) {
  if (bytes >= 1e9) return (bytes / 1e9).toFixed(1) + ' GB';
  if (bytes >= 1e6) return (bytes / 1e6).toFixed(1) + ' MB';
  if (bytes >= 1e3) return (bytes / 1e3).toFixed(1) + ' KB';
  return bytes + ' B';
}
