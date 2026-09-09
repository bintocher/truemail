// truemail UI module: popup-menus.js
// Чистая логика правила "одно вспомогательное меню одновременно" (S-020):
// на вход - идентификаторы открытых меню и описание действия пользователя,
// на выход - какие меню остаются открытыми, какие закрываются и надо ли просто
// переставить уже открытое меню к новой точке курсора.
// Модуль не обращается к дереву страницы и к глобальному объекту окна: узлы
// ищут модули интерфейса.
// См. specs/single-popup-menu.md.

// Идентификаторы семейства. Порядок задаёт порядок закрытия и ничего больше.
const POPUP_MENU_IDS = [
  'message', 'smart', 'folder', 'tag', 'contact',
  'attachment', 'flag', 'more', 'filter', 'sort', 'icon', 'color',
];

// Единственная разрешённая пара: меню письма и его подменю меток, открытое
// наведением указателя (S-001, S-006).
const SUBMENU_PARENT = { flag: 'message' };

// Решение по действию пользователя.
// open - идентификаторы открытых сейчас меню;
// action - {type:'open', id, sameTarget, hover} | {type:'escape'} | {type:'outside'}.
// Возвращает {open, close, reposition}: open - что останется открытым,
// close - что закрыть, reposition - показать уже открытое меню на новом месте.
function planPopupMenus(open, action) {
  const current = [...new Set((open || []).filter(id => POPUP_MENU_IDS.includes(id)))];
  const type = action?.type;
  if (type === 'escape' || type === 'outside') {
    return { open: [], close: current, reposition: false };
  }
  if (type !== 'open') return { open: current, close: [], reposition: false };
  const id = action.id;
  if (!POPUP_MENU_IDS.includes(id)) return { open: current, close: [], reposition: false };
  // Подменю меток, открытое наведением, оставляет родительское меню письма.
  const keepParent = action.hover && SUBMENU_PARENT[id] && current.includes(SUBMENU_PARENT[id])
    ? SUBMENU_PARENT[id]
    : null;
  // Открытие родителя заново не должно гасить его подменю в паре: подменю
  // закрывается вместе с родителем, а не при его перестановке (S-004, S-008).
  const keepSubmenu = action.sameTarget
    ? Object.keys(SUBMENU_PARENT).filter(sub => SUBMENU_PARENT[sub] === id && current.includes(sub))
    : [];
  const keep = [id, keepParent, ...keepSubmenu].filter(Boolean);
  const close = current.filter(other => !keep.includes(other));
  const wasOpen = current.includes(id);
  return {
    open: [...new Set(keep)],
    close,
    reposition: Boolean(action.sameTarget && wasOpen),
  };
}

// Меню, которое остаётся открытым вместе с закрываемым: закрытие меню письма
// закрывает и подменю меток (S-008). Возвращает идентификаторы, которые должны
// уйти вместе с перечисленными.
function withDependentMenus(ids) {
  const result = [...new Set(ids || [])];
  Object.keys(SUBMENU_PARENT).forEach(sub => {
    if (result.includes(SUBMENU_PARENT[sub]) && !result.includes(sub)) result.push(sub);
  });
  return result;
}

const popupMenus = { POPUP_MENU_IDS, SUBMENU_PARENT, planPopupMenus, withDependentMenus };
if (typeof module !== 'undefined' && module.exports) module.exports = popupMenus;
