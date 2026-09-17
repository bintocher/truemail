// truemail UI module: account-cards.js
// Чистые функции без DOM и Tauri API: переключение аккордеона карточек
// аккаунтов (не более одной раскрытой) и проверка, показывать ли кнопку
// "Сменить пароль" для данного способа входа. Подключается в index.html как
// обычный скрипт, перед settings.js, и отдаёт функции через глобальный объект.
// См. specs/accounts-accordion-password.md.

// Переключение аккордеона (S-002): клик по свёрнутой карточке раскрывает её
// и сворачивает прежнюю; клик по уже раскрытой - сворачивает, раскрытых не
// остаётся. currentId/clickedId сравниваются как есть (Number или null).
function nextOpenAccountId(currentId, clickedId) {
  return currentId === clickedId ? null : clickedId;
}

// Восстановление раскрытой карточки из localStorage (S-001, S-003, S-005):
// пустое/испорченное значение и id отсутствующего аккаунта дают null - тогда
// раскрытых карточек нет и запись хранилища подлежит очистке вызывающей
// стороной. accountIds - массив id аккаунтов текущего списка.
function restoreOpenAccountId(savedValue, accountIds) {
  if (savedValue === null || savedValue === undefined || savedValue === '') return null;
  const id = Number(savedValue);
  if (!Number.isFinite(id)) return null;
  const known = Array.isArray(accountIds) && accountIds.some(item => Number(item) === id);
  return known ? id : null;
}

// Кнопка "Сменить пароль" - только у парольных аккаунтов (S-008). OAuth,
// отсутствующее и неизвестное значение auth_kind её не получают.
function canChangeAccountPassword(authKind) {
  return authKind === 'password' || authKind === 'app_password' || authKind === 'ntlm';
}

function isOAuthAccount(authKind) {
  return authKind === 'oauth2';
}

function russianCountForm(value, one, few, many) {
  const count = Math.abs(Number(value)) || 0;
  const mod100 = count % 100;
  const mod10 = count % 10;
  if (mod100 >= 11 && mod100 <= 14) return many;
  if (mod10 === 1) return one;
  if (mod10 >= 2 && mod10 <= 4) return few;
  return many;
}

// Статистика карточки строится без DOM, чтобы формы слов для обоих языков
// проверялись отдельно от отрисовки настроек.
function accountStatsText(folderCount, calendarCount, locale) {
  const folders = Number(folderCount) || 0;
  const calendars = Number(calendarCount) || 0;
  if (locale === 'en') {
    return `${folders} ${folders === 1 ? 'folder' : 'folders'} · ${calendars} ${calendars === 1 ? 'calendar' : 'calendars'}`;
  }
  return `${folders} ${russianCountForm(folders, 'папка', 'папки', 'папок')} · ${calendars} ${russianCountForm(calendars, 'календарь', 'календаря', 'календарей')}`;
}

const accountCards = {
  nextOpenAccountId,
  restoreOpenAccountId,
  canChangeAccountPassword,
  isOAuthAccount,
  accountStatsText,
};
if (typeof module !== 'undefined' && module.exports) module.exports = accountCards;
