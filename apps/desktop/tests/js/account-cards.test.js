// Проверки чистой логики apps/desktop/ui/modules/account-cards.js.
// Запуск: node --test apps/desktop/tests/js/account-cards.test.js (Node 20+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {
  nextOpenAccountId,
  restoreOpenAccountId,
  canChangeAccountPassword,
  isOAuthAccount,
  accountStatsText,
} = require('../../ui/modules/account-cards.js');

// S-002: переключение аккордеона.
test('S-002: первый клик раскрывает карточку', () => {
  assert.equal(nextOpenAccountId(null, 1), 1);
});
test('S-002: переход A -> B закрывает A и открывает B', () => {
  assert.equal(nextOpenAccountId(1, 2), 2);
});
test('S-002: повторный клик по раскрытой B сворачивает её', () => {
  assert.equal(nextOpenAccountId(2, 2), null);
});

// S-001, S-003, S-005: восстановление состояния из localStorage.
test('S-001: пустое хранилище - раскрытых карточек нет', () => {
  assert.equal(restoreOpenAccountId(null, [1, 2, 3]), null);
  assert.equal(restoreOpenAccountId(undefined, [1, 2, 3]), null);
  assert.equal(restoreOpenAccountId('', [1, 2, 3]), null);
});
test('S-003: существующий id восстанавливается', () => {
  assert.equal(restoreOpenAccountId('2', [1, 2, 3]), 2);
  assert.equal(restoreOpenAccountId(2, [1, 2, 3]), 2);
});
test('S-003: отсутствующий id дает null', () => {
  assert.equal(restoreOpenAccountId('99', [1, 2, 3]), null);
});
test('S-005: испорченное значение дает null, а не исключение', () => {
  assert.equal(restoreOpenAccountId('not-a-number', [1, 2, 3]), null);
  assert.equal(restoreOpenAccountId('{"broken":true}', [1, 2, 3]), null);
  assert.equal(restoreOpenAccountId('1', []), null);
  assert.equal(restoreOpenAccountId('1', null), null);
});

// S-008: кнопка "Сменить пароль" только у парольных аккаунтов.
test('S-008: canChangeAccountPassword по всем значениям auth_kind', () => {
  assert.equal(canChangeAccountPassword('password'), true);
  assert.equal(canChangeAccountPassword('app_password'), true);
  assert.equal(canChangeAccountPassword('ntlm'), true);
  assert.equal(canChangeAccountPassword('oauth2'), false);
  assert.equal(canChangeAccountPassword(undefined), false);
  assert.equal(canChangeAccountPassword(null), false);
  assert.equal(canChangeAccountPassword('unknown_kind'), false);
  assert.equal(canChangeAccountPassword(''), false);
});

test('issue 79: число папок и календарей согласовано по-русски', () => {
  assert.equal(accountStatsText(1, 1, 'ru'), '1 папка · 1 календарь');
  assert.equal(accountStatsText(2, 2, 'ru'), '2 папки · 2 календаря');
  assert.equal(accountStatsText(8, 8, 'ru'), '8 папок · 8 календарей');
  assert.equal(accountStatsText(21, 14, 'ru'), '21 папка · 14 календарей');
  // G6: 5-20 (кроме особого случая 11-14 ниже) - форма "много".
  assert.equal(accountStatsText(5, 5, 'ru'), '5 папок · 5 календарей');
  // 11-14 - особый случай "много" даже несмотря на оканчание на "1"-"4".
  assert.equal(accountStatsText(11, 11, 'ru'), '11 папок · 11 календарей');
  // 101 оканчивается на 1, но не входит в 11-14 - снова форма "один".
  assert.equal(accountStatsText(101, 101, 'ru'), '101 папка · 101 календарь');
});

test('issue 79: английское множественное число зависит от единицы', () => {
  assert.equal(accountStatsText(1, 2, 'en'), '1 folder · 2 calendars');
  assert.equal(accountStatsText(3, 1, 'en'), '3 folders · 1 calendar');
});

test('issue 79: повторный вход показывается только для OAuth', () => {
  assert.equal(isOAuthAccount('oauth2'), true);
  assert.equal(isOAuthAccount('password'), false);
  assert.equal(isOAuthAccount('app_password'), false);
  assert.equal(isOAuthAccount('ntlm'), false);
});
