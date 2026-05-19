import { renderCodeBlock } from './CodeBlock.js';
import { generateSnippet } from '../wasm-bridge.js';

/**
 * Renders the snippet panel with engine tabs and bundle selector.
 * Returns the panel element with setActiveBundle() and setActiveEngine() methods.
 */
export function renderSnippetPanel(container, report, initialBundle = 'a') {
  const panel = document.createElement('div');
  panel.id = 'snippet-panel';
  panel.className = 'bg-canvas-subtle rounded-md border border-border-default overflow-hidden';

  panel.innerHTML = `
    <div class="px-5 py-4 border-b border-border-muted flex flex-wrap gap-2 items-center justify-between">
      <h3 class="text-sm font-semibold text-fg-default">Code Snippet</h3>
      <div class="flex gap-1" id="snippet-engine-tabs">
        <button class="snippet-engine-tab px-3 py-1 text-xs rounded-md transition-colors" data-engine="pyarrow">PyArrow</button>
        <button class="snippet-engine-tab px-3 py-1 text-xs rounded-md transition-colors" data-engine="pyspark">PySpark</button>
        <button class="snippet-engine-tab px-3 py-1 text-xs rounded-md transition-colors" data-engine="polars">Polars</button>
      </div>
    </div>
    <div class="px-5 py-3 border-b border-border-muted flex gap-2 flex-wrap" id="snippet-bundle-tabs">
      <span class="text-xs text-fg-muted self-center mr-1">Bundle:</span>
    </div>
    <div class="p-4">
      <div id="snippet-code-area" class="min-h-[120px]">
        <div class="flex items-center justify-center h-24 text-fg-muted text-sm">Loading...</div>
      </div>
      <div id="snippet-caveat" class="mt-3 text-xs text-fg-muted italic"></div>
    </div>
  `;

  container.appendChild(panel);

  let activeBundle = initialBundle;
  let activeEngine = 'pyarrow';

  const engineTabs = panel.querySelectorAll('.snippet-engine-tab');
  const bundleTabsContainer = panel.querySelector('#snippet-bundle-tabs');
  const codeArea = panel.querySelector('#snippet-code-area');
  const caveatEl = panel.querySelector('#snippet-caveat');

  // Build bundle tabs from report.options
  const bundleKeys = Object.keys(report.options);
  bundleKeys.forEach(key => {
    const opt = report.options[key];
    const btn = document.createElement('button');
    btn.className = 'snippet-bundle-tab px-3 py-1 text-xs rounded transition-colors';
    btn.dataset.bundle = key;
    btn.textContent = opt.label;
    bundleTabsContainer.appendChild(btn);
  });

  async function getSnippet() {
    try {
      const engineKey = activeEngine === 'pyarrow' ? 'pyarrow' : activeEngine;
      return await generateSnippet(report, engineKey);
    } catch {
      // Fallback to cached snippet in report if WASM call fails
      return report.options?.[activeBundle]?.python_snippet ?? report.python_snippet ?? '# Snippet unavailable';
    }
  }

  function getCaveat() {
    if (activeEngine === 'pyspark') {
      return 'ZSTD requires Spark 3.2+. Per-column encoding hints require Spark 3.4+.';
    }
    if (activeEngine === 'polars') {
      return 'Polars does not support per-column encoding hints via the Python API as of Polars 0.20.';
    }
    return 'All predicted values are [estimated]. Run autoparq bench to validate before rewriting large files.';
  }

  function updateActiveTabs() {
    // Engine tabs
    engineTabs.forEach(tab => {
      if (tab.dataset.engine === activeEngine) {
        tab.className = 'snippet-engine-tab px-3 py-1 text-xs rounded-md bg-accent-emphasis text-fg-on-emphasis';
      } else {
        tab.className = 'snippet-engine-tab px-3 py-1 text-xs rounded-md bg-canvas-default text-fg-muted border border-border-default hover:bg-canvas-inset transition-colors';
      }
    });

    // Bundle tabs
    const bundleBtns = bundleTabsContainer.querySelectorAll('.snippet-bundle-tab');
    bundleBtns.forEach(btn => {
      if (btn.dataset.bundle === activeBundle) {
        btn.className = 'snippet-bundle-tab px-3 py-1 text-xs rounded-md bg-accent-emphasis text-fg-on-emphasis';
      } else {
        btn.className = 'snippet-bundle-tab px-3 py-1 text-xs rounded-md bg-canvas-default text-fg-muted border border-border-default hover:bg-canvas-inset transition-colors';
      }
    });

    // Hide bundle tabs when not pyarrow
    bundleBtns.forEach(btn => {
      btn.style.display = activeEngine === 'pyarrow' ? '' : 'none';
    });
    const bundleLabel = bundleTabsContainer.querySelector('span');
    if (bundleLabel) bundleLabel.style.display = activeEngine === 'pyarrow' ? '' : 'none';
  }

  async function refresh() {
    updateActiveTabs();
    codeArea.innerHTML = '<div class="flex items-center justify-center h-24 text-fg-muted text-sm">Generating snippet...</div>';
    caveatEl.textContent = getCaveat();
    const snippet = await getSnippet();
    await renderCodeBlock(codeArea, snippet);
  }

  // Event listeners
  engineTabs.forEach(tab => {
    tab.addEventListener('click', () => {
      activeEngine = tab.dataset.engine;
      refresh();
    });
  });

  bundleTabsContainer.addEventListener('click', (e) => {
    const btn = e.target.closest('.snippet-bundle-tab');
    if (!btn) return;
    activeBundle = btn.dataset.bundle;
    refresh();
  });

  // Expose methods
  panel.setActiveBundle = (bundleKey) => {
    activeBundle = bundleKey;
    activeEngine = 'pyarrow';
    refresh();
    // Scroll to panel
    panel.scrollIntoView({ behavior: 'smooth', block: 'start' });
  };

  panel.setActiveEngine = (engineStr) => {
    activeEngine = engineStr;
    refresh();
  };

  // Initial render
  refresh();

  return panel;
}
