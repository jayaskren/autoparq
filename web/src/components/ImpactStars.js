/**
 * Returns a span element with filled ★ and empty ☆ stars based on count (0-5).
 * Uses GitHub Primer attention-emphasis gold for filled; muted gray for empty.
 */
export function ImpactStars(count) {
  const span = document.createElement('span');
  span.setAttribute('aria-label', `Impact: ${count} out of 5 stars`);
  span.className = 'font-mono text-sm';

  let html = '';
  for (let i = 1; i <= 5; i++) {
    if (i <= count) {
      html += `<span style="color:#bf8700" aria-hidden="true">★</span>`;
    } else {
      html += `<span style="color:#afb8c1" aria-hidden="true">☆</span>`;
    }
  }
  span.innerHTML = html;
  return span;
}
