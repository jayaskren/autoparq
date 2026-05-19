import { renderReport } from './render/report.js';
import { renderEngineSelector, renderPrioritySelector, showProgress, hideProgress, showError } from './ui.js';
import { analyzeFile as wasmAnalyze, reRecommend, checkFileSizeWasm, benchColumn, applyFile } from './wasm-bridge.js';

// ------- App state -------
let _engineSelect = null;
let _prioritySelect = null;
let _currentReport = null;
let _currentFileName = null;
let _analyzing = false;

// Spec 008 — apply/download state
let _rewriteResult = null;       // { rewrite, perColumnDiff, engine, priority, outputBytes }
let _rewriteOutputBytes = null;  // Uint8Array held for re-download
let _rewriteDiscardTimer = null; // setTimeout handle for auto-discard
let _applying = false;           // gates engine/priority selects during apply

const REWRITE_DISCARD_MS = 60_000;

// ------- Public API -------

export function init() {
  // Render engine/priority selectors
  const engineSlot = document.getElementById('engine-select-container');
  const prioritySlot = document.getElementById('priority-select-container');

  if (engineSlot) _engineSelect = renderEngineSelector(engineSlot);
  if (prioritySlot) _prioritySelect = renderPrioritySelector(prioritySlot);

  // Compact engine selector in sticky header
  const headerSlot = document.getElementById('engine-selector-slot');
  if (headerSlot) {
    const headerEngine = document.createElement('select');
    headerEngine.className = 'hidden bg-gray-800 border border-gray-700 text-gray-200 text-xs rounded px-2 py-1 focus:outline-none focus:border-indigo-500';
    headerEngine.id = 'header-engine-select';
    const engines = ['unknown','duckdb','spark','polars','clickhouse','pandas'];
    const labels =  ['Engine','DuckDB','Spark','Polars','ClickHouse','Pandas'];
    engines.forEach((val, i) => {
      const opt = document.createElement('option');
      opt.value = val;
      opt.textContent = labels[i];
      headerEngine.appendChild(opt);
    });
    headerSlot.appendChild(headerEngine);
    headerEngine.addEventListener('change', () => {
      if (_engineSelect) _engineSelect.value = headerEngine.value;
      onEngineChange();
    });
    if (_engineSelect) {
      _engineSelect.addEventListener('change', () => {
        headerEngine.value = _engineSelect.value;
        onEngineChange();
      });
    }
  }

  if (_prioritySelect) {
    _prioritySelect.addEventListener('change', () => onEngineChange());
  }

  // Drop zone
  const dropZone = document.getElementById('drop-zone');
  const fileInput = document.getElementById('file-input');

  if (dropZone) {
    dropZone.addEventListener('click', (e) => {
      if (e.target !== fileInput) fileInput?.click();
    });
    dropZone.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') fileInput?.click();
    });
    dropZone.addEventListener('dragover', (e) => {
      e.preventDefault();
      dropZone.classList.add('border-indigo-500', 'bg-indigo-950/20');
      dropZone.classList.remove('border-gray-600');
    });
    dropZone.addEventListener('dragleave', (e) => {
      if (!dropZone.contains(e.relatedTarget)) {
        setDropZoneIdle();
      }
    });
    dropZone.addEventListener('drop', (e) => {
      e.preventDefault();
      setDropZoneIdle();
      const file = e.dataTransfer?.files?.[0];
      if (file) handleFile(file);
    });
  }

  if (fileInput) {
    fileInput.addEventListener('change', () => {
      const file = fileInput.files?.[0];
      if (file) handleFile(file);
      fileInput.value = '';
    });
  }

  // Sample file buttons
  document.querySelectorAll('[data-sample-url]').forEach((btn) => {
    btn.addEventListener('click', () => handleSampleFile(btn.dataset.sampleUrl, btn.dataset.sampleName));
  });

  // Reset handler
  document.addEventListener('autoparq:reset', reset);

  // Re-download resets the auto-discard timer
  document.addEventListener('autoparq:reset-discard-timer', () => {
    scheduleRewriteDiscard();
  });
}

function clearRewriteState() {
  _rewriteResult = null;
  _rewriteOutputBytes = null;
  if (_rewriteDiscardTimer) {
    clearTimeout(_rewriteDiscardTimer);
    _rewriteDiscardTimer = null;
  }
}

function scheduleRewriteDiscard() {
  if (_rewriteDiscardTimer) clearTimeout(_rewriteDiscardTimer);
  _rewriteDiscardTimer = setTimeout(() => {
    _rewriteOutputBytes = null;
    if (_rewriteResult) _rewriteResult.outputBytes = null;
    _rewriteDiscardTimer = null;
    // Re-render so the success strip shows "Re-apply to download".
    const reportContainer = document.getElementById('report-container');
    if (reportContainer && _currentReport) {
      renderReport(reportContainer, _currentReport, _currentFileName, buildRenderOpts());
    }
  }, REWRITE_DISCARD_MS);
}

function setEngineDisabled(disabled) {
  _applying = disabled;
  const ids = ['engine-select', 'priority-select', 'header-engine-select'];
  for (const id of ids) {
    const el = document.getElementById(id);
    if (el) el.disabled = disabled;
  }
}

function buildRenderOpts() {
  return {
    benchColumnFn: benchColumn,
    onApply: async (engine, priority) => {
      setEngineDisabled(true);
      try {
        const res = await applyFile(engine, priority);
        _rewriteResult = {
          rewrite: res.rewrite,
          perColumnDiff: res.perColumnDiff,
          engine,
          priority,
          outputBytes: res.outputBytes,
        };
        _rewriteOutputBytes = res.outputBytes;
        scheduleRewriteDiscard();
        const reportContainer = document.getElementById('report-container');
        if (reportContainer) {
          renderReport(reportContainer, _currentReport, _currentFileName, buildRenderOpts());
        }
        return res;
      } finally {
        setEngineDisabled(false);
      }
    },
    getRewriteState: () => _rewriteResult,
    onDiscard: () => {
      clearRewriteState();
      const reportContainer = document.getElementById('report-container');
      if (reportContainer && _currentReport) {
        renderReport(reportContainer, _currentReport, _currentFileName, buildRenderOpts());
      }
    },
  };
}

export function reset() {
  _currentReport = null;
  _currentFileName = null;
  _analyzing = false;
  clearRewriteState();

  const hero = document.getElementById('hero');
  const progress = document.getElementById('progress');
  const report = document.getElementById('report');
  const reportContainer = document.getElementById('report-container');
  const headerEngineSelect = document.getElementById('header-engine-select');

  if (hero) hero.classList.remove('hidden');
  if (progress) progress.classList.add('hidden');
  if (report) report.classList.add('hidden');
  if (reportContainer) reportContainer.innerHTML = '';
  if (headerEngineSelect) headerEngineSelect.classList.add('hidden');

  setDropZoneIdle();
  clearDropZoneError();
}

// ------- Internal -------

function setDropZoneIdle() {
  const dropZone = document.getElementById('drop-zone');
  if (!dropZone) return;
  dropZone.classList.remove('border-indigo-500', 'bg-indigo-950/20', 'border-red-500', 'bg-red-950/10');
  dropZone.classList.add('border-gray-600');
}

function setDropZoneError(msg) {
  const dropZone = document.getElementById('drop-zone');
  const errEl = document.getElementById('drop-zone-error');
  if (dropZone) {
    dropZone.classList.remove('border-gray-600', 'border-indigo-500', 'bg-indigo-950/20');
    dropZone.classList.add('border-red-500', 'bg-red-950/10');
  }
  if (errEl) {
    errEl.textContent = msg;
    errEl.classList.remove('hidden');
  }
  setTimeout(() => {
    setDropZoneIdle();
    clearDropZoneError();
  }, 3000);
}

function clearDropZoneError() {
  const errEl = document.getElementById('drop-zone-error');
  if (errEl) {
    errEl.classList.add('hidden');
    errEl.textContent = '';
  }
}

function getOptions() {
  return {
    engine: _engineSelect?.value ?? 'unknown',
    priority: _prioritySelect?.value ?? 'balanced',
  };
}

async function handleFile(file) {
  if (_analyzing) return;
  clearDropZoneError();
  clearRewriteState();

  if (!file.name.endsWith('.parquet')) {
    setDropZoneError('Only .parquet files are supported.');
    return;
  }

  let sizeCheck;
  try {
    sizeCheck = await checkFileSizeWasm(file.size);
  } catch {
    sizeCheck = { ok: true, warning: null, error: null };
  }

  if (sizeCheck.error) {
    setDropZoneError(sizeCheck.error);
    return;
  }

  if (sizeCheck.warning) {
    showFileSizeWarning(sizeCheck.warning);
  }

  const hero = document.getElementById('hero');
  const progress = document.getElementById('progress');

  if (hero) hero.classList.add('hidden');
  if (progress) progress.classList.remove('hidden');

  _analyzing = true;
  const options = getOptions();
  _currentFileName = file.name;

  try {
    const arrayBuffer = await file.arrayBuffer();
    const report = await wasmAnalyze(arrayBuffer, options, (current, total, colName) => {
      const pct = total > 0 ? Math.round((current / total) * 80) + 10 : 10;
      const label = colName ? `Sampling column: ${colName}` : 'Analyzing…';
      showProgress(label, pct);
    });

    showProgress('Rendering report…', 98);
    _currentReport = report;
    hideProgress();
    showReport(report, file.name);
  } catch (err) {
    hideProgress();
    showError('Analysis Failed', err?.message ?? 'An unexpected error occurred.');
  } finally {
    _analyzing = false;
  }
}

async function handleSampleFile(url, name) {
  if (_analyzing) return;

  const hero = document.getElementById('hero');
  const progress = document.getElementById('progress');
  if (hero) hero.classList.add('hidden');
  if (progress) progress.classList.remove('hidden');

  _analyzing = true;
  _currentFileName = name ?? url.split('/').pop();

  try {
    showProgress('Fetching sample file…', 5);
    const resp = await fetch(url);
    if (!resp.ok) throw new Error(`Failed to fetch sample: ${resp.statusText}`);
    const arrayBuffer = await resp.arrayBuffer();

    const options = getOptions();
    const report = await wasmAnalyze(arrayBuffer, options, (current, total, colName) => {
      const pct = total > 0 ? Math.round((current / total) * 80) + 10 : 10;
      const label = colName ? `Sampling column: ${colName}` : 'Analyzing…';
      showProgress(label, pct);
    });

    showProgress('Rendering report…', 98);
    _currentReport = report;
    hideProgress();
    showReport(report, _currentFileName);
  } catch (err) {
    hideProgress();
    showError('Sample Load Failed', err?.message ?? 'Could not load sample file.');
  } finally {
    _analyzing = false;
  }
}

function showFileSizeWarning(msg) {
  const errEl = document.getElementById('drop-zone-error');
  if (errEl) {
    errEl.textContent = msg;
    errEl.classList.remove('hidden');
  }
}

function showReport(report, fileName) {
  const hero = document.getElementById('hero');
  const reportSection = document.getElementById('report');
  const reportContainer = document.getElementById('report-container');
  const headerEngineSelect = document.getElementById('header-engine-select');

  if (hero) hero.classList.add('hidden');
  if (reportSection) reportSection.classList.remove('hidden');
  if (reportContainer) {
    renderReport(reportContainer, report, fileName, buildRenderOpts());
  }

  if (headerEngineSelect) {
    headerEngineSelect.classList.remove('hidden');
    headerEngineSelect.value = _engineSelect?.value ?? 'unknown';
  }

  window.scrollTo({ top: 0, behavior: 'smooth' });
}

async function onEngineChange() {
  if (!_currentReport || !_currentFileName || _analyzing || _applying) return;

  // Engine/priority change invalidates any measured-apply overlay (Spec 008 A10)
  clearRewriteState();

  const options = getOptions();
  const reportContainer = document.getElementById('report-container');

  try {
    const newReport = await reRecommend(_currentReport, options.engine, options.priority);
    _currentReport = newReport;
    if (reportContainer) {
      renderReport(reportContainer, newReport, _currentFileName, buildRenderOpts());
    }
  } catch (err) {
    console.error('Engine change re-recommend failed:', err);
    if (reportContainer) {
      renderReport(reportContainer, _currentReport, _currentFileName, buildRenderOpts());
    }
  }
}
