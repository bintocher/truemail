// Выполняется до первой отрисовки, чтобы сохранённая тёмная тема не мигала
// светлой. Значение не чувствительное; каноническая настройка остаётся в БД.
try {
  const theme = localStorage.getItem("truemail-theme");
  if (theme === "dark" || theme === "light") {
    document.documentElement.dataset.theme = theme;
  }
} catch (_) {}
