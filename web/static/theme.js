(() => {
  const storageKey = 'azure-sql-tco-theme';
  const root = document.documentElement;
  let storedTheme = null;

  try {
    storedTheme = localStorage.getItem(storageKey);
  } catch {
    storedTheme = null;
  }

  const theme = storedTheme === 'light' || storedTheme === 'dark' ? storedTheme : 'dark';

  root.dataset.theme = theme;
  root.style.colorScheme = theme;
  document
    .querySelector('meta[name="theme-color"]')
    ?.setAttribute('content', theme === 'dark' ? '#0f171c' : '#eef2f3');
})();
