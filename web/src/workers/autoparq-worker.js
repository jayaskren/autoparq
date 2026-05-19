import init, {
  tune_file_bytes_with_progress,
  recommend_from_profile,
  generate_snippet,
  check_file_size,
  bench_column_bytes,
  apply_file_bytes,
  check_apply_file_size,
} from '@wasm/autoparq.js';

let wasmReady = false;
let _lastFileData = null; // retained after analyze for bench calls

async function ensureWasm() {
  if (!wasmReady) {
    await init();
    wasmReady = true;
  }
}

self.onmessage = async (e) => {
  const { id, type, payload } = e.data;

  try {
    await ensureWasm();

    if (type === 'analyze') {
      const { buffer, engine, priority, sampleRows } = payload;
      const data = new Uint8Array(buffer);
      _lastFileData = data; // retain for bench calls

      const onProgress = (current, total, colName) => {
        self.postMessage({ id, type: 'progress', payload: { current, total, colName } });
      };

      const reportJson = tune_file_bytes_with_progress(data, engine, priority, sampleRows, onProgress);
      self.postMessage({ id, type: 'result', payload: { reportJson } });

    } else if (type === 'recommend') {
      const { reportJson, engine, priority } = payload;
      const newReportJson = recommend_from_profile(reportJson, engine, priority);
      self.postMessage({ id, type: 'result', payload: { reportJson: newReportJson } });

    } else if (type === 'snippet') {
      const { reportJson, engine } = payload;
      const snippet = generate_snippet(reportJson, engine);
      self.postMessage({ id, type: 'result', payload: { snippet } });

    } else if (type === 'checkSize') {
      const { byteLen } = payload;
      const result = check_file_size(byteLen);
      self.postMessage({ id, type: 'result', payload: { result } });

    } else if (type === 'bench') {
      const { columnName } = payload;
      if (!_lastFileData) throw new Error('No file loaded. Run analysis first.');
      const resultJson = bench_column_bytes(_lastFileData, columnName);
      self.postMessage({ id, type: 'result', payload: { resultJson } });

    } else if (type === 'apply') {
      const { engine, priority } = payload;
      if (!_lastFileData) throw new Error('No file loaded. Run analysis first.');
      const result = apply_file_bytes(_lastFileData, engine, priority);
      const outputBytes = result.output;   // Uint8Array
      const summaryJson = result.summary;  // string
      self.postMessage(
        { id, type: 'result', payload: { outputBytes, summaryJson } },
        [outputBytes.buffer],
      );

    } else if (type === 'checkApplySize') {
      const { byteLen } = payload;
      const result = check_apply_file_size(byteLen);
      self.postMessage({ id, type: 'result', payload: { result } });

    } else {
      throw new Error(`Unknown message type: ${type}`);
    }
  } catch (err) {
    self.postMessage({ id, type: 'error', payload: { message: err?.message ?? String(err) } });
  }
};
