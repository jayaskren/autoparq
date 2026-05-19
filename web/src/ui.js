/**
 * UI utilities: selectors, progress, error panel.
 */

/**
 * Builds an engine <select> element and appends to container.
 * Returns the select element.
 */
export function renderEngineSelector(container) {
  const select = document.createElement('select');
  select.id = 'engine-select';
  select.className = 'bg-gray-800 border border-gray-700 text-gray-200 text-sm rounded px-3 py-1.5 focus:outline-none focus:border-indigo-500';

  const engines = [
    { value: 'unknown', label: 'Unknown engine' },
    { value: 'duckdb',     label: 'DuckDB' },
    { value: 'spark',      label: 'Spark' },
    { value: 'polars',     label: 'Polars' },
    { value: 'clickhouse', label: 'ClickHouse' },
    { value: 'pandas',     label: 'Pandas' },
  ];

  for (const eng of engines) {
    const opt = document.createElement('option');
    opt.value = eng.value;
    opt.textContent = eng.label;
    select.appendChild(opt);
  }

  container.appendChild(select);
  return select;
}

/**
 * Builds a priority <select> element and appends to container.
 * Returns the select element.
 */
export function renderPrioritySelector(container) {
  const select = document.createElement('select');
  select.id = 'priority-select';
  select.className = 'bg-gray-800 border border-gray-700 text-gray-200 text-sm rounded px-3 py-1.5 focus:outline-none focus:border-indigo-500';

  const priorities = [
    { value: 'balanced', label: 'Balanced' },
    { value: 'size',     label: 'Smallest size' },
    { value: 'speed',    label: 'Fastest reads' },
  ];

  for (const p of priorities) {
    const opt = document.createElement('option');
    opt.value = p.value;
    opt.textContent = p.label;
    select.appendChild(opt);
  }

  container.appendChild(select);
  return select;
}

/**
 * Updates the progress bar and label.
 * Unhides #progress if it was hidden.
 */
export function showProgress(message, pct) {
  const progressSection = document.getElementById('progress');
  const bar = document.getElementById('progress-bar');
  const label = document.getElementById('progress-label');

  if (progressSection) progressSection.classList.remove('hidden');
  if (bar) bar.style.width = `${Math.min(100, Math.max(0, pct))}%`;
  if (label) label.textContent = message;
}

/**
 * Hides the progress section.
 */
export function hideProgress() {
  const progressSection = document.getElementById('progress');
  if (progressSection) progressSection.classList.add('hidden');
}

/**
 * Renders an error panel in the #report area.
 * Includes an icon, title, message, and "Analyze another file" button.
 */
export function showError(title, message) {
  const reportSection = document.getElementById('report');
  if (!reportSection) return;

  reportSection.classList.remove('hidden');

  const container = document.getElementById('report-container');
  if (!container) return;

  container.innerHTML = `
    <div class="max-w-xl mx-auto py-16 text-center">
      <div class="text-5xl mb-4">⚠️</div>
      <h2 class="text-xl font-bold text-red-400 mb-2">${escHtml(title)}</h2>
      <p class="text-gray-400 mb-6">${escHtml(message)}</p>
      <button id="error-reset-btn"
        class="px-5 py-2 bg-indigo-600 hover:bg-indigo-700 text-white rounded-lg transition-colors text-sm">
        ← Analyze another file
      </button>
    </div>
  `;

  const resetBtn = container.querySelector('#error-reset-btn');
  if (resetBtn) {
    resetBtn.addEventListener('click', () => {
      document.dispatchEvent(new CustomEvent('autoparq:reset'));
    });
  }
}

function escHtml(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
