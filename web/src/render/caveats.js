const CAVEAT_REFS = [
  {
    keywords: ['zstd', 'spark'],
    label: 'SPARK-25366',
    url: 'https://issues.apache.org/jira/browse/SPARK-25366',
  },
  {
    keywords: ['high-entropy'],
    label: 'entropy heuristic',
    url: 'https://btrfs.readthedocs.io/en/latest/Compression.html',
  },
];

function caveatRefLink(message) {
  const lower = message.toLowerCase();
  const match = CAVEAT_REFS.find(r => r.keywords.every(k => lower.includes(k)));
  if (!match) return '';
  return ` — <a href="${match.url}" target="_blank" rel="noopener noreferrer"
    class="underline underline-offset-2 hover:opacity-80">${match.label}</a>`;
}

/**
 * Renders aggregated caveats from file-level and per-column sources.
 */
export function renderCaveats(container, report) {
  const allCaveats = [];

  // File-level caveats
  if (report.file_caveats) {
    for (const c of report.file_caveats) {
      allCaveats.push({ severity: c.severity, message: c.message, source: null });
    }
  }

  // Per-column caveats
  for (const col of report.columns) {
    if (col.caveats && col.caveats.length > 0) {
      for (const c of col.caveats) {
        const msg = typeof c === 'string' ? c : c.message ?? String(c);
        const sev = typeof c === 'object' ? (c.severity ?? 'Warning') : 'Warning';
        allCaveats.push({ severity: sev, message: msg, source: col.column_name });
      }
    }
  }

  if (allCaveats.length === 0) return;

  const section = document.createElement('section');
  section.id = 'section-caveats';

  section.innerHTML = `<h2 class="text-lg font-semibold text-fg-default mb-4">Notes &amp; Caveats</h2>`;

  const list = document.createElement('ul');
  list.className = 'space-y-2';

  for (const caveat of allCaveats) {
    const isWarning = caveat.severity === 'Warning';
    const icon = isWarning ? '⚠' : 'ℹ';
    const colorClass = isWarning
      ? 'text-attention-fg bg-attention-subtle border-attention-emphasis/30'
      : 'text-accent-fg bg-accent-subtle border-accent-emphasis/30';

    const li = document.createElement('li');
    li.className = `flex items-start gap-2 rounded-md border px-4 py-3 text-sm ${colorClass}`;

    li.innerHTML = `
      <span class="text-base mt-0.5 shrink-0">${icon}</span>
      <div>
        ${caveat.source ? `<span class="font-mono text-xs opacity-70 block mb-0.5">column: ${caveat.source}</span>` : ''}
        <span>${caveat.message}${caveatRefLink(caveat.message)}</span>
      </div>
    `;
    list.appendChild(li);
  }

  section.appendChild(list);
  container.appendChild(section);
}
