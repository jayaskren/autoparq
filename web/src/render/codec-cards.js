const CODEC_REFS = {
  ZSTD:         { label: 'zstd benchmarks', url: 'https://github.com/facebook/zstd' },
  LZ4:          { label: 'LZ4 benchmarks', url: 'https://lz4.org/' },
  LZ4_RAW:      { label: 'LZ4 benchmarks', url: 'https://lz4.org/' },
  SNAPPY:       { label: 'Spark Parquet docs', url: 'https://spark.apache.org/docs/latest/sql-data-sources-parquet.html' },
  UNCOMPRESSED: { label: 'entropy heuristic', url: 'https://btrfs.readthedocs.io/en/latest/Compression.html' },
};

/**
 * Renders the three codec bundle cards (Balanced, Smallest, Fastest).
 * Returns an object with setActiveBundle(key) method.
 */
export function renderCodecCards(container, report, snippetPanelRef) {
  const section = document.createElement('section');
  section.id = 'section-codecs';

  section.innerHTML = `<h2 class="text-lg font-semibold text-fg-default mb-4">Compression Options</h2>`;

  const grid = document.createElement('div');
  grid.className = 'grid grid-cols-1 md:grid-cols-3 gap-4';

  const icons = { a: '⚖️', b: '🗜️', c: '⚡' };

  Object.entries(report.options).forEach(([key, opt]) => {
    const isRecommended = key === 'a';

    const card = document.createElement('div');
    card.id = `codec-card-${key}`;
    card.className = [
      'rounded-md border p-5 flex flex-col gap-3 transition-all duration-200',
      isRecommended
        ? 'border-accent-emphasis bg-accent-subtle'
        : 'border-border-default bg-canvas-subtle',
    ].join(' ');

    const header = document.createElement('div');
    header.className = 'flex items-start justify-between';

    const titleArea = document.createElement('div');
    titleArea.innerHTML = `
      <div class="flex items-center gap-2">
        <span class="text-2xl">${icons[key] ?? '📦'}</span>
        <span class="font-semibold text-fg-default">${opt.label}</span>
      </div>
      <div class="font-mono text-xs text-fg-muted mt-1">${opt.codec_description}</div>
    `;

    const badge = document.createElement('div');
    if (isRecommended) {
      badge.className = 'text-xs bg-accent-emphasis text-fg-on-emphasis rounded-full px-2 py-0.5 font-semibold';
      badge.textContent = 'RECOMMENDED';
    }

    header.append(titleArea, badge);

    const tradeoff = document.createElement('p');
    tradeoff.className = 'text-sm text-fg-muted flex-1';
    tradeoff.textContent = opt.tradeoff;

    const caveats = opt.caveats && opt.caveats.length > 0
      ? opt.caveats.map(c => `<li class="text-xs text-attention-fg">⚠ ${c}</li>`).join('')
      : '';

    const caveatList = document.createElement('ul');
    caveatList.className = 'space-y-1';
    caveatList.innerHTML = caveats;

    const baseCodec = (opt.codec_description ?? '').split(':')[0].toUpperCase();
    const codecRef = CODEC_REFS[baseCodec];
    const whyLink = document.createElement('p');
    whyLink.className = 'text-xs text-fg-muted';
    if (codecRef) {
      whyLink.innerHTML = `<a href="${codecRef.url}" target="_blank" rel="noopener noreferrer"
        class="text-accent-fg hover:underline underline-offset-2">why ${baseCodec}? →</a>`;
    }

    const snippetBtn = document.createElement('button');
    snippetBtn.className = [
      'mt-auto text-xs px-3 py-1.5 rounded-md border transition-colors',
      isRecommended
        ? 'border-accent-emphasis text-accent-fg hover:bg-canvas-default'
        : 'border-border-default text-fg-default hover:bg-canvas-inset',
    ].join(' ');
    snippetBtn.textContent = 'Get snippet →';
    snippetBtn.addEventListener('click', () => {
      if (snippetPanelRef && snippetPanelRef.current) {
        snippetPanelRef.current.setActiveBundle(key);
      }
    });

    card.append(header, tradeoff, whyLink, caveatList, snippetBtn);
    grid.appendChild(card);
  });

  section.appendChild(grid);
  container.appendChild(section);
}
