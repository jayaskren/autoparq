/**
 * Render a recommendation's codec + level as a single string (e.g., "ZSTD:3").
 * If no level is set, returns the codec name only.
 */
export function fullCodec(col) {
  return col.recommended_codec_level != null
    ? `${col.recommended_codec}:${col.recommended_codec_level}`
    : col.recommended_codec;
}

/**
 * Compute the file-wide recommended codec as the mode (most frequent)
 * across all column recommendations.
 */
export function modeCodec(columns) {
  if (!columns || columns.length === 0) return null;
  const counts = new Map();
  for (const c of columns) {
    const full = fullCodec(c);
    counts.set(full, (counts.get(full) ?? 0) + 1);
  }
  let best = null;
  let bestCount = -1;
  for (const [codec, count] of counts) {
    if (count > bestCount) {
      best = codec;
      bestCount = count;
    }
  }
  return best;
}
