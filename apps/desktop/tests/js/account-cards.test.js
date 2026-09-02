// Проверки чистой логики apps/desktop/ui/modules/account-cards.js.
// Запуск: node --test apps/desktop/tests/js/account-cards.test.js (Node 20+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {nextOpenAccountId, restoreOpenAccountId, canChangeAccountPassword} = require('../../ui/modules/account-cards.js');

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
