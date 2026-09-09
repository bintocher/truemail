// truemail UI module: list-render-hold.js
// Чистая логика отсрочки перестроения окна списка писем на время нажатия.
// Строки списка пересоздаются целиком, и замена узла между прижатием и
// отпусканием кнопки съедает нажатие: письмо не открывается (issue #61).
// Модуль не обращается к дереву страницы и к глобальному объекту окна.
// См. specs/message-click-not-lost.md.

// Решение по запросу на перестроение окна.
// held - идёт ли удержание указателя на списке;
// force - запрошено принудительное перестроение;
// pending - что уже отложено: null, если ничего.
// Возвращает {render, pending}: render - перестраивать ли сейчас и с каким
// признаком (null - не перестраивать), pending - новое отложенное состояние.
function planWindowRender(held, force, pending) {
  // Признак накапливается и при немедленном перестроении: между концом
  // удержания и отложенной задачей успевает пройти обычный запрос, и
  // принудительность отложенного иначе пропала бы.
  if (!held) {
    return { render: { force: Boolean(force) || Boolean(pending && pending.force) }, pending: null };
  }
  // Признак принудительного перестроения накапливается: одно принудительное
  // среди нескольких отложенных делает принудительным и общий результат.
  const merged = { force: Boolean(force) || Boolean(pending && pending.force) };
  return { render: null, pending: merged };
}

// Решение по завершению удержания: что выполнить и с каким признаком.
// Возвращает {render, pending} по тем же правилам, что и planWindowRender.
function releaseWindowRender(pending) {
  if (!pending) return { render: null, pending: null };
  return { render: { force: Boolean(pending.force) }, pending: null };
}

const listRenderHold = { planWindowRender, releaseWindowRender };
if (typeof module !== 'undefined' && module.exports) module.exports = listRenderHold;
