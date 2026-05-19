export function ConfidenceBadge(tier) {
  const config = {
    High:   { bg: 'bg-success-subtle',   text: 'text-success-fg',   label: 'HIGH' },
    Medium: { bg: 'bg-attention-subtle', text: 'text-attention-fg', label: 'MED'  },
    Low:    { bg: 'bg-danger-subtle',    text: 'text-danger-fg',    label: 'LOW'  },
  };
  const c = config[tier] ?? config.Low;
  const span = document.createElement('span');
  span.className = `${c.bg} ${c.text} rounded-full px-2 py-0.5 text-xs font-semibold border border-border-default`;
  span.textContent = c.label;
  return span;
}
