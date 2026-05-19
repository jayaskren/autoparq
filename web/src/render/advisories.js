function refLink(label, url) {
  return `<a href="${url}" target="_blank" rel="noopener noreferrer"
     class="text-accent-fg hover:underline underline-offset-2 transition-colors">${label}</a>`;
}

/**
 * Renders Row Group and Sort Order advisories when applicable.
 */
export function renderAdvisories(container, report) {
  const section = document.createElement('section');
  section.id = 'section-advisories';

  const rga = report.row_group_advisory;
  const sa = report.sort_advisory;

  const hasRowGroup = rga && !rga.is_within_recommendation;
  const hasSort = sa && sa.inferred_sort_candidates && sa.inferred_sort_candidates.length > 0;

  if (!hasRowGroup && !hasSort) return;

  section.innerHTML = `<h2 class="text-lg font-semibold text-fg-default mb-4">Advisories</h2>`;

  if (hasRowGroup) {
    const avgMB = rga.current_avg_mb.toFixed(1);
    const recMinMB = rga.recommended_range_mb[0].toFixed(0);
    const recMaxMB = rga.recommended_range_mb[1].toFixed(0);
    const numGroups = report.file_profile?.num_row_groups ?? '—';

    const panel = document.createElement('div');
    panel.className = 'bg-attention-subtle border border-attention-emphasis/30 rounded-md p-5 mb-4';
    panel.innerHTML = `
      <div class="flex items-start gap-3">
        <span class="text-attention-fg text-xl mt-0.5">⚠</span>
        <div>
          <h3 class="font-semibold text-attention-fg mb-2">Row Group Size Advisory</h3>
          <p class="text-sm text-fg-default mb-3">${rga.advice}</p>
          <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
            <div class="bg-canvas-default border border-attention-emphasis/20 rounded-md p-2 text-center">
              <div class="font-mono text-lg text-attention-fg">${numGroups}</div>
              <div class="text-xs text-fg-muted">Row groups</div>
            </div>
            <div class="bg-canvas-default border border-attention-emphasis/20 rounded-md p-2 text-center">
              <div class="font-mono text-lg text-attention-fg">${avgMB} MB</div>
              <div class="text-xs text-fg-muted">Avg group size</div>
            </div>
            <div class="bg-canvas-default border border-attention-emphasis/20 rounded-md p-2 text-center">
              <div class="font-mono text-lg text-attention-fg">${recMinMB}–${recMaxMB} MB</div>
              <div class="text-xs text-fg-muted">Recommended range</div>
            </div>
            <div class="bg-canvas-default border border-attention-emphasis/20 rounded-md p-2 text-center">
              <div class="font-mono text-lg text-attention-fg">${rga.workload_label}</div>
              <div class="text-xs text-fg-muted">Workload</div>
            </div>
          </div>
          <p class="mt-3 text-xs text-fg-muted">
            Sources:
            ${refLink('Apache Parquet spec', 'https://parquet.apache.org/docs/file-format/configurations/')}
            ·
            ${refLink('Spark docs', 'https://spark.apache.org/docs/latest/sql-data-sources-parquet.html')}
            ·
            ${refLink('DuckDB performance guide', 'https://duckdb.org/docs/current/guides/performance/file_formats')}
            ·
            ${refLink('Delta Lake docs', 'https://docs.databricks.com/aws/en/delta/tune-file-size')}
          </p>
        </div>
      </div>
    `;
    section.appendChild(panel);
  }

  if (hasSort) {
    const panel = document.createElement('div');
    panel.className = 'bg-accent-subtle border border-accent-emphasis/30 rounded-md p-5';
    panel.innerHTML = `
      <div class="flex items-start gap-3">
        <span class="text-accent-fg text-xl mt-0.5">ℹ</span>
        <div>
          <h3 class="font-semibold text-accent-fg mb-2">Sort Order Advisory</h3>
          <p class="text-sm text-fg-default mb-3">${sa.advice}</p>
          <div class="flex flex-wrap gap-2">
            <span class="text-xs text-fg-muted self-center">Inferred sort columns:</span>
            ${sa.inferred_sort_candidates.map(c =>
              `<span class="font-mono text-xs bg-canvas-default text-accent-fg border border-accent-emphasis/30 rounded-full px-2 py-0.5">${c}</span>`
            ).join('')}
          </div>
          ${sa.declared_sort_columns && sa.declared_sort_columns.length > 0 ? `
            <div class="flex flex-wrap gap-2 mt-2">
              <span class="text-xs text-fg-muted self-center">Declared in footer:</span>
              ${sa.declared_sort_columns.map(c =>
                `<span class="font-mono text-xs bg-canvas-default text-fg-muted border border-border-default rounded-full px-2 py-0.5">${c}</span>`
              ).join('')}
            </div>
          ` : ''}
        </div>
      </div>
    `;
    section.appendChild(panel);
  }

  container.appendChild(section);
}
