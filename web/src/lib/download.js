/**
 * Trigger a browser download from in-memory bytes. No network involved.
 */
export function triggerDownload(bytes, suggestedName) {
  const blob = new Blob([bytes], { type: 'application/octet-stream' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = suggestedName;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  setTimeout(() => URL.revokeObjectURL(url), 2000);
}

/**
 * Derive a tuned filename from the original, preserving any .parquet extension.
 * `events.parquet` → `events_tuned.parquet`
 */
export function tunedFilename(originalName) {
  if (!originalName) return 'output_tuned.parquet';
  const idx = originalName.lastIndexOf('.');
  const base = idx > 0 ? originalName.slice(0, idx) : originalName;
  const ext = idx > 0 ? originalName.slice(idx) : '.parquet';
  return `${base}_tuned${ext}`;
}
