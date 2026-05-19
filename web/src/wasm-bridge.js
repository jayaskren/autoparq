let _worker = null;
let _pendingCalls = new Map();
let _callId = 0;

function getWorker() {
  if (!_worker) {
    _worker = new Worker(new URL('./workers/autoparq-worker.js', import.meta.url), { type: 'module' });
    _worker.onmessage = (e) => {
      const { id, type, payload } = e.data;
      const pending = _pendingCalls.get(id);
      if (!pending) return;

      if (type === 'progress') {
        pending.onProgress?.(payload.current, payload.total, payload.colName);
      } else if (type === 'result') {
        _pendingCalls.delete(id);
        pending.resolve(payload);
      } else if (type === 'error') {
        _pendingCalls.delete(id);
        pending.reject(new Error(payload.message));
      }
    };
    _worker.onerror = (e) => {
      for (const [id, pending] of _pendingCalls) {
        pending.reject(new Error(e.message ?? 'Worker error'));
      }
      _pendingCalls.clear();
      _worker = null;
    };
  }
  return _worker;
}

function call(type, payload, onProgress, transferables = []) {
  return new Promise((resolve, reject) => {
    const id = ++_callId;
    _pendingCalls.set(id, { resolve, reject, onProgress });
    getWorker().postMessage({ id, type, payload }, transferables);
  });
}

/**
 * Analyze a parquet file. The ArrayBuffer is transferred to the Worker (zero-copy).
 * onProgress(current, total, colName) is called per column.
 * Returns a parsed TuneReport object.
 */
export async function analyzeFile(arrayBuffer, options, onProgress) {
  const { reportJson } = await call(
    'analyze',
    {
      buffer: arrayBuffer,
      engine: options.engine ?? 'unknown',
      priority: options.priority ?? 'balanced',
      sampleRows: options.sampleRows ?? 2_000_000,
    },
    onProgress,
    [arrayBuffer]  // transfer ownership to Worker — zero-copy
  );
  return JSON.parse(reportJson);
}

/**
 * Re-run recommendations from cached profile data (engine/priority change).
 * Much faster than re-analyzing the file — skips profiling entirely.
 * Returns a parsed TuneReport object.
 */
export async function reRecommend(report, engine, priority) {
  const { reportJson } = await call('recommend', {
    reportJson: JSON.stringify(report),
    engine,
    priority,
  });
  return JSON.parse(reportJson);
}

/**
 * Generate a code snippet for the given engine without re-analyzing.
 * Returns the snippet string.
 */
export async function generateSnippet(report, engine) {
  const { snippet } = await call('snippet', {
    reportJson: JSON.stringify(report),
    engine,
  });
  return snippet;
}

/**
 * Check whether the file size is acceptable.
 * Returns { ok, warning, error } parsed from JSON.
 */
export async function checkFileSizeWasm(byteLen) {
  const { result } = await call('checkSize', { byteLen });
  return JSON.parse(result);
}

/**
 * Benchmark a column using all default codecs and valid encodings for its type.
 * Returns a parsed BenchResult object.
 * Requires that analyzeFile has been called first (file bytes are cached in the worker).
 */
export async function benchColumn(columnName) {
  const { resultJson } = await call('bench', { columnName });
  return JSON.parse(resultJson);
}

/**
 * Apply recommendations in WASM and return the rewritten parquet bytes
 * plus a per-column before/after delta. Zero server involvement.
 * Requires that analyzeFile has been called first (file bytes are cached in the worker).
 */
export async function applyFile(engine, priority) {
  const { outputBytes, summaryJson } = await call('apply', { engine, priority });
  const summary = JSON.parse(summaryJson);
  return {
    outputBytes,
    rewrite: summary.rewrite,
    perColumnDiff: summary.per_column_diff,
  };
}

/**
 * Check whether the file size is acceptable for in-browser apply.
 * Returns { ok, warning, severity, error } parsed from JSON.
 * severity: 'blocked' | 'severe' | 'mild' | null
 */
export async function checkApplySize(byteLen) {
  const { result } = await call('checkApplySize', { byteLen });
  return JSON.parse(result);
}
