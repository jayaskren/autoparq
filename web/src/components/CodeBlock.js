// Lazy-loads Shiki for syntax highlighting
let _highlighter = null;

async function getHighlighter() {
  if (_highlighter) return _highlighter;
  const { createHighlighter } = await import('shiki');
  _highlighter = await createHighlighter({ themes: ['github-light'], langs: ['python'] });
  return _highlighter;
}

export async function renderCodeBlock(container, code) {
  const hl = await getHighlighter();
  const html = hl.codeToHtml(code, { lang: 'python', theme: 'github-light' });
  const wrapper = document.createElement('div');
  wrapper.className = 'relative rounded-md overflow-hidden text-sm max-h-96 overflow-y-auto border border-border-muted';
  wrapper.innerHTML = html;

  const copyBtn = document.createElement('button');
  copyBtn.className = 'absolute top-2 right-2 bg-canvas-subtle hover:bg-canvas-inset text-fg-default border border-border-default text-xs px-2 py-1 rounded-md transition-colors';
  copyBtn.textContent = 'Copy';
  copyBtn.addEventListener('click', async () => {
    try {
      if (navigator.clipboard && window.isSecureContext) {
        await navigator.clipboard.writeText(code);
      } else {
        // Fallback for http:// (python3 -m http.server)
        const ta = document.createElement('textarea');
        ta.value = code;
        ta.style.position = 'fixed';
        ta.style.opacity = '0';
        document.body.appendChild(ta);
        ta.select();
        document.execCommand('copy');
        document.body.removeChild(ta);
      }
      copyBtn.textContent = 'Copied!';
      setTimeout(() => { copyBtn.textContent = 'Copy'; }, 2000);
    } catch (e) {
      copyBtn.textContent = 'Failed';
    }
  });
  wrapper.appendChild(copyBtn);
  container.replaceChildren(wrapper);
}
