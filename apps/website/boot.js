/* Applied before first paint so the stored theme never flashes. */
(function () {
  try {
    var q = new URLSearchParams(location.search);
    var theme = q.get('theme') || localStorage.getItem('warsha.theme') || 'system';
    var dark = theme === 'dark' || (theme === 'system' && matchMedia('(prefers-color-scheme: dark)').matches);
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    document.documentElement.dataset.themeChoice = theme;
  } catch (e) {
    document.documentElement.dataset.theme = 'light';
    document.documentElement.dataset.themeChoice = 'system';
  }
})();