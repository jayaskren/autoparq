import { renderSummary } from './summary.js';
import { renderColumns } from './columns.js';
import { renderCodecCards } from './codec-cards.js';
import { renderAdvisories } from './advisories.js';
import { renderCaveats } from './caveats.js';
import { renderSnippetPanel } from '../components/SnippetPanel.js';
import { applyGlossaryMarkup, initTooltips } from '../components/Tooltip.js';

/**
 * Orchestrates the full report render into `container`.
 *
 * @param {HTMLElement} container
 * @param {object} report
 * @param {string} fileName
 * @param {object} [opts]
 * @param {Function} [opts.benchColumnFn]   — async (columnName) => BenchResult
 * @param {Function} [opts.onApply]         — async (engine, priority) => apply result
 * @param {Function} [opts.getRewriteState] — () => { rewrite, perColumnDiff, engine, priority } | null
 * @param {Function} [opts.onDiscard]       — () => clears rewrite state, re-renders
 */
export function renderReport(container, report, fileName, opts = {}) {
  const {
    benchColumnFn,
    onApply,
    getRewriteState,
    onDiscard,
  } = opts;

  container.innerHTML = '';

  // -- Report header --
  const header = document.createElement('div');
  header.className = 'flex flex-wrap items-center justify-between gap-4 mb-8 pb-5 border-b border-border-default';

  const fileInfo = document.createElement('div');
  fileInfo.innerHTML = `
    <h1 class="text-xl font-semibold text-fg-default font-mono">${escHtml(fileName)}</h1>
    <div class="flex flex-wrap gap-4 mt-1 text-sm text-fg-muted">
      <span>${formatBytes(report.file_size_bytes)}</span>
      <span>·</span>
      <span>${report.num_rows.toLocaleString()} rows</span>
      <span>·</span>
      <span>${report.num_columns} columns</span>
      <span>·</span>
      <span>Engine: <span class="font-mono text-accent-fg">${report.engine}</span></span>
      <span>·</span>
      <span>Priority: <span class="font-mono text-accent-fg">${report.priority}</span></span>
    </div>
  `;

  const analyzeAgainBtn = document.createElement('button');
  analyzeAgainBtn.id = 'analyze-again-btn';
  analyzeAgainBtn.className = 'text-sm px-4 py-2 rounded-md border border-border-default text-fg-default bg-canvas-subtle hover:bg-canvas-inset transition-colors';
  analyzeAgainBtn.textContent = '← Analyze another file';
  analyzeAgainBtn.addEventListener('click', () => {
    // App.reset() will be called via a custom event
    document.dispatchEvent(new CustomEvent('autoparq:reset'));
  });

  header.append(fileInfo, analyzeAgainBtn);
  container.appendChild(header);

  // -- Desktop layout: two-column (nav rail + content) --
  const layout = document.createElement('div');
  layout.className = 'flex gap-8';

  // Nav rail (sticky, desktop only)
  const navSections = [
    { id: 'section-summary', label: 'Summary' },
    { id: 'section-columns', label: 'Columns' },
    { id: 'section-codecs', label: 'Options' },
    { id: 'snippet-panel', label: 'Snippet' },
    { id: 'section-advisories', label: 'Advisories' },
    { id: 'section-caveats', label: 'Caveats' },
  ];

  const nav = document.createElement('nav');
  nav.className = 'hidden lg:block w-44 shrink-0';
  nav.innerHTML = `
    <div class="sticky top-20 space-y-1">
      ${navSections.map(s => `
        <a href="#${s.id}"
           class="block text-sm text-fg-muted hover:text-accent-fg px-3 py-1.5 rounded-md hover:bg-canvas-subtle transition-colors"
           onclick="event.preventDefault(); document.getElementById('${s.id}')?.scrollIntoView({behavior:'smooth', block:'start'})">
          ${s.label}
        </a>
      `).join('')}
    </div>
  `;

  // Main content
  const main = document.createElement('div');
  main.className = 'flex-1 min-w-0 space-y-10';

  layout.append(nav, main);
  container.appendChild(layout);

  // Snippet panel reference (so codec cards can trigger it)
  const snippetPanelRef = { current: null };

  // -- Render sections --
  renderSummary(main, report, {
    onApply,
    getRewriteState,
    onDiscard,
    fileName,
  });
  renderColumns(main, report, {
    benchColumnFn,
    perColumnDiff: getRewriteState?.()?.perColumnDiff,
  });
  renderCodecCards(main, report, snippetPanelRef);

  // Snippet panel
  const snippetWrapper = document.createElement('section');
  const snippetEl = renderSnippetPanel(snippetWrapper, report, 'a');
  snippetPanelRef.current = snippetEl;
  main.appendChild(snippetWrapper);

  renderAdvisories(main, report);
  renderCaveats(main, report);

  // -- Glossary tooltips --
  applyGlossaryMarkup(main);
  initTooltips(main);
}

function formatBytes(bytes) {
  if (bytes >= 1e9) return (bytes / 1e9).toFixed(1) + ' GB';
  if (bytes >= 1e6) return (bytes / 1e6).toFixed(1) + ' MB';
  if (bytes >= 1e3) return (bytes / 1e3).toFixed(1) + ' KB';
  return bytes + ' B';
}

function escHtml(str) {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
