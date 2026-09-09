// Проверки отсрочки перестроения окна списка писем.
// Спецификация: specs/message-click-not-lost.md.
// Запуск: node --test apps/desktop/tests/js/list-render-hold.test.js (Node 20+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {planWindowRender, releaseWindowRender} = require('../../ui/modules/list-render-hold.js');

// S-007: без удержания перестроение идёт сразу, как до задачи.
test('S-007: без удержания перестраиваем сразу', () => {
  const plan = planWindowRender(false, false, null);
  assert.deepEqual(plan.render, {force: false});
  assert.equal(plan.pending, null);
});
test('S-007: признак принудительного перестроения доходит без удержания', () => {
  assert.deepEqual(planWindowRender(false, true, null).render, {force: true});
});
test('S-005: без удержания накопленный признак не теряется', () => {
  const plan = planWindowRender(false, false, {force: true});
  assert.deepEqual(plan.render, {force: true});
  assert.equal(plan.pending, null);
});

// S-002: во время удержания окно не трогаем.
test('S-002: во время удержания перестроения нет', () => {
  const plan = planWindowRender(true, false, null);
  assert.equal(plan.render, null);
  assert.deepEqual(plan.pending, {force: false});
});

// S-004: несколько запросов за удержание дают одно перестроение.
test('S-004: запросы за удержание сливаются в один', () => {
  let pending = null;
  for (let i = 0; i < 5; i++) pending = planWindowRender(true, false, pending).pending;
  const release = releaseWindowRender(pending);
  assert.deepEqual(release.render, {force: false});
  assert.equal(release.pending, null);
});

// S-005: одно принудительное среди отложенных делает принудительным результат.
test('S-005: принудительное перестроение не теряется', () => {
  let pending = planWindowRender(true, false, null).pending;
  pending = planWindowRender(true, true, pending).pending;
  pending = planWindowRender(true, false, pending).pending;
  assert.deepEqual(releaseWindowRender(pending).render, {force: true});
});

// S-003, S-006: без отложенного запроса конец удержания ничего не запускает.
test('S-003: конец удержания без запросов ничего не перестраивает', () => {
  const release = releaseWindowRender(null);
  assert.equal(release.render, null);
  assert.equal(release.pending, null);
});
