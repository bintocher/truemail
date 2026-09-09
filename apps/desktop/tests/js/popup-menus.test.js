// Проверки правила "одно вспомогательное меню одновременно".
// Спецификация: specs/single-popup-menu.md.
// Запуск: node --test apps/desktop/tests/js/popup-menus.test.js (Node 20+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {POPUP_MENU_IDS, planPopupMenus, withDependentMenus} = require('../../ui/modules/popup-menus.js');

// S-001, S-002: открытие любого меню закрывает все прочие.
test('S-002: открытие меню папки закрывает меню письма', () => {
  const plan = planPopupMenus(['message'], {type: 'open', id: 'folder'});
  assert.deepEqual(plan.open, ['folder']);
  assert.deepEqual(plan.close, ['message']);
  assert.equal(plan.reposition, false);
});
test('S-002: закрываются меню с любым признаком открытого состояния', () => {
  const plan = planPopupMenus(['filter', 'attachment', 'more', 'color'], {type: 'open', id: 'sort'});
  assert.deepEqual(plan.open, ['sort']);
  assert.deepEqual(plan.close.sort(), ['attachment', 'color', 'filter', 'more']);
});
test('S-001: результат никогда не содержит двух меню вне пары', () => {
  POPUP_MENU_IDS.forEach(id => {
    const plan = planPopupMenus(POPUP_MENU_IDS, {type: 'open', id});
    assert.deepEqual(plan.open, [id]);
  });
});

// S-003: правый клик по обычной папке закрывает ранее открытое меню.
test('S-003: меню умной папки закрывается при открытии меню папки', () => {
  const plan = planPopupMenus(['smart'], {type: 'open', id: 'folder'});
  assert.deepEqual(plan.close, ['smart']);
});

// S-004: повторный правый клик по тому же элементу переставляет меню.
test('S-004: повторный клик по тому же элементу не закрывает меню', () => {
  const plan = planPopupMenus(['message'], {type: 'open', id: 'message', sameTarget: true});
  assert.equal(plan.reposition, true);
  assert.deepEqual(plan.close, []);
  assert.deepEqual(plan.open, ['message']);
});
test('S-004: клик по другому элементу того же вида меню не считается повторным', () => {
  const plan = planPopupMenus(['message'], {type: 'open', id: 'message', sameTarget: false});
  assert.equal(plan.reposition, false);
});

// S-006: подменю меток, открытое наведением, оставляет родителя.
test('S-006: при наведении меню письма остаётся открытым', () => {
  const plan = planPopupMenus(['message'], {type: 'open', id: 'flag', hover: true});
  assert.deepEqual(plan.open.sort(), ['flag', 'message']);
  assert.deepEqual(plan.close, []);
});
test('S-007: без наведения подменю остаётся единственным', () => {
  const plan = planPopupMenus(['message'], {type: 'open', id: 'flag', hover: false});
  assert.deepEqual(plan.open, ['flag']);
  assert.deepEqual(plan.close, ['message']);
});
test('S-006: наведение закрывает прочие меню, кроме родителя', () => {
  const plan = planPopupMenus(['message', 'filter'], {type: 'open', id: 'flag', hover: true});
  assert.deepEqual(plan.close, ['filter']);
});

// S-008: закрытие меню письма закрывает подменю меток.
test('S-008: Escape закрывает пару меню и подменю', () => {
  const plan = planPopupMenus(['message', 'flag'], {type: 'escape'});
  assert.deepEqual(plan.open, []);
  assert.deepEqual(plan.close.sort(), ['flag', 'message']);
});
test('S-008: закрытие меню письма тянет за собой подменю', () => {
  assert.deepEqual(withDependentMenus(['message']).sort(), ['flag', 'message']);
  assert.deepEqual(withDependentMenus(['folder']), ['folder']);
});
test('S-008: открытие другого меню закрывает и подменю', () => {
  const plan = planPopupMenus(['message', 'flag'], {type: 'open', id: 'folder'});
  assert.deepEqual(plan.close.sort(), ['flag', 'message']);
});

// S-011, S-014: Escape и клик вне меню закрывают всё открытое.
test('S-011: Escape закрывает меню всех видов', () => {
  const plan = planPopupMenus(['tag', 'contact', 'attachment', 'more', 'icon', 'color'], {type: 'escape'});
  assert.deepEqual(plan.open, []);
  assert.equal(plan.close.length, 6);
});
test('S-014: клик вне меню закрывает всё открытое', () => {
  const plan = planPopupMenus(['attachment'], {type: 'outside'});
  assert.deepEqual(plan.close, ['attachment']);
});
test('S-011: без открытых меню закрывать нечего', () => {
  assert.deepEqual(planPopupMenus([], {type: 'escape'}).close, []);
});

// S-016: фильтр и сортировка исключают друг друга.
test('S-016: сортировка закрывает фильтр', () => {
  assert.deepEqual(planPopupMenus(['filter'], {type: 'open', id: 'sort'}).close, ['filter']);
  assert.deepEqual(planPopupMenus(['sort'], {type: 'open', id: 'filter'}).close, ['sort']);
});

// S-019: неизвестные идентификаторы не ломают разбор.
test('S-019: неизвестное меню в списке открытых игнорируется', () => {
  const plan = planPopupMenus(['message', 'нетТакого'], {type: 'escape'});
  assert.deepEqual(plan.close, ['message']);
});
