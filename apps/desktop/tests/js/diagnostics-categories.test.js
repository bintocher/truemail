// Проверки apps/desktop/ui/modules/diagnostics-categories.js.
// Запуск: node --test apps/desktop/tests/js/diagnostics-categories.test.js (Node 20+).
'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {DIAGNOSTICS_CATEGORY_KEYS}=require('../../ui/modules/diagnostics-categories.js');

// S-014: список категорий перед сбором - статичный набор, не пустой.
test('S-014: список категорий не пуст и без повторов',()=>{
  assert.ok(DIAGNOSTICS_CATEGORY_KEYS.length>=5);
  assert.equal(new Set(DIAGNOSTICS_CATEGORY_KEYS).size,DIAGNOSTICS_CATEGORY_KEYS.length);
});
