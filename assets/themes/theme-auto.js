// Call early in <head> to avoid a theme flash. Safe without storage access.
(() => {
  let theme = 'dark';
  try { theme = localStorage.getItem('bitdinghy-theme') || theme; } catch (_) {}
  if (theme === 'auto') theme = matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
  document.documentElement.dataset.theme = theme === 'light' ? 'light' : 'dark';
})();
