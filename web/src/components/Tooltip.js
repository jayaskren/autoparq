const GLOSSARY = {
  'DELTA_BINARY_PACKED': 'Stores differences between consecutive values instead of full values. Highly effective for monotonically increasing integers and timestamps. Rule fires when monotonicity_score ≥ 0.90.',
  'RLE_DICTIONARY': 'Stores each distinct value once in a dictionary, then replaces every occurrence with a small integer index. Ideal for low-cardinality columns (few distinct values).',
  'BYTE_STREAM_SPLIT': 'Rearranges the bytes of floating-point values so similar byte positions are grouped together, giving the codec more repetition to compress. Fires for FLOAT/DOUBLE columns with cardinality > 50%.',
  'PLAIN': 'Stores values as-is with no transformation. The safe baseline encoding with no overhead.',
  'cardinality_ratio': 'The proportion of distinct values to total rows. 0.001 means 0.1% unique values — very low cardinality, ideal for dictionary encoding.',
  'monotonicity_score': 'Fraction of consecutive pairs where the value is non-decreasing. 1.0 = perfectly ascending. Score ≥ 0.90 triggers DELTA_BINARY_PACKED.',
  'byte_entropy': 'Shannon entropy of the byte distribution (0–8 bits/byte). Values > 7.5 indicate pre-compressed or random data that will not compress further.',
  'UNCOMPRESSED': 'No codec applied. Used when byte_entropy is very high, meaning compression would actually increase file size.',
  'ZSTD': 'Zstandard compression. Excellent size/speed balance. Level 3 is the default; level 6 gives smaller files at the cost of slower writes.',
  'LZ4': 'LZ4 compression. Fastest decompression of any codec. Files are ~20-30% larger than ZSTD:3 but read faster.',
  'SNAPPY': 'Snappy compression. Fast and widely supported, especially in older Spark versions. Not as small as ZSTD.',
};

let _activeTooltip = null;
let _hideTimeout = null;

function createTooltipEl(text) {
  const tip = document.createElement('div');
  tip.className = 'glossary-tooltip';
  tip.textContent = text;
  document.body.appendChild(tip);
  return tip;
}

function positionTooltip(tip, anchorEl) {
  const rect = anchorEl.getBoundingClientRect();
  const tipWidth = 280;
  const margin = 8;

  let left = rect.left;
  let top = rect.bottom + margin;

  // Prevent overflow off right edge
  if (left + tipWidth > window.innerWidth - margin) {
    left = window.innerWidth - tipWidth - margin;
  }
  if (left < margin) left = margin;

  // Flip above if too close to bottom
  if (top + 80 > window.innerHeight) {
    top = rect.top - margin - 80;
  }

  tip.style.left = `${left}px`;
  tip.style.top = `${top}px`;
}

function showTooltip(anchorEl, term) {
  clearTimeout(_hideTimeout);

  if (_activeTooltip) {
    _activeTooltip.remove();
    _activeTooltip = null;
  }

  const text = GLOSSARY[term];
  if (!text) return;

  const tip = createTooltipEl(text);
  _activeTooltip = tip;
  positionTooltip(tip, anchorEl);

  // Make tooltip interactive so cursor can move into it
  tip.classList.add('interactive');
  tip.addEventListener('mouseenter', () => clearTimeout(_hideTimeout));
  tip.addEventListener('mouseleave', () => {
    _hideTimeout = setTimeout(hideTooltip, 100);
  });
}

function hideTooltip() {
  if (_activeTooltip) {
    _activeTooltip.remove();
    _activeTooltip = null;
  }
}

export function initTooltips(rootElement) {
  // Escape key closes tooltip
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') hideTooltip();
  });

  rootElement.addEventListener('mouseover', (e) => {
    const el = e.target.closest('[data-glossary]');
    if (!el) return;
    const term = el.dataset.glossary;
    showTooltip(el, term);
  });

  rootElement.addEventListener('mouseout', (e) => {
    const el = e.target.closest('[data-glossary]');
    if (!el) return;
    // Delay hide to allow moving into tooltip
    _hideTimeout = setTimeout(hideTooltip, 100);
  });
}

/**
 * Walks text nodes in element, finds glossary terms, wraps in
 * <span data-glossary="term"> with dotted underline.
 * Only wraps terms that appear as standalone words.
 */
export function applyGlossaryMarkup(element) {
  const terms = Object.keys(GLOSSARY);
  // Sort longest first to avoid partial matches
  terms.sort((a, b) => b.length - a.length);

  // Build regex that matches whole terms (word-boundary aware for underscored terms)
  const escapedTerms = terms.map(t =>
    t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  );
  const pattern = new RegExp(`(${escapedTerms.join('|')})`, 'g');

  walkTextNodes(element, pattern);
}

function walkTextNodes(node, pattern) {
  if (node.nodeType === Node.TEXT_NODE) {
    const text = node.textContent;
    if (!pattern.test(text)) return;
    pattern.lastIndex = 0;

    const frag = document.createDocumentFragment();
    let lastIndex = 0;
    let match;

    while ((match = pattern.exec(text)) !== null) {
      if (match.index > lastIndex) {
        frag.appendChild(document.createTextNode(text.slice(lastIndex, match.index)));
      }
      const span = document.createElement('span');
      span.dataset.glossary = match[0];
      span.textContent = match[0];
      frag.appendChild(span);
      lastIndex = pattern.lastIndex;
    }

    if (lastIndex < text.length) {
      frag.appendChild(document.createTextNode(text.slice(lastIndex)));
    }

    node.parentNode.replaceChild(frag, node);
    return;
  }

  // Skip script, style, and already-glossary-marked elements
  if (
    node.nodeType === Node.ELEMENT_NODE &&
    (node.tagName === 'SCRIPT' ||
      node.tagName === 'STYLE' ||
      node.hasAttribute('data-glossary') ||
      node.tagName === 'CODE' ||
      node.tagName === 'PRE')
  ) {
    return;
  }

  // Walk children (snapshot to avoid mutation issues)
  const children = Array.from(node.childNodes);
  for (const child of children) {
    walkTextNodes(child, pattern);
  }
}
