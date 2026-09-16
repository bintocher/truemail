// truemail UI module: diagnostics-categories.js
// Статичный список категорий обезличивания для предупреждения перед сбором
// диагностики (specs/diagnostics-bundle.md, S-014, раздел "Интерфейсы и
// данные": список хранится в интерфейсе и не требует отдельной команды).
// Порядок соответствует терминам спецификации: email и host отдельно, path,
// folder, а перечисленные идентификаторы (account_id, message_id, folder_id,
// uuid, uid, message_id_header) - одной понятной пользователю строкой.
'use strict';

const DIAGNOSTICS_CATEGORY_KEYS = Object.freeze([
  'diagnosticsCategoryEmail',
  'diagnosticsCategoryHost',
  'diagnosticsCategoryPath',
  'diagnosticsCategoryFolder',
  'diagnosticsCategoryIds',
]);

const diagnosticsCategories = { DIAGNOSTICS_CATEGORY_KEYS };
if (typeof window !== 'undefined') window.diagnosticsCategories = diagnosticsCategories;
if (typeof module !== 'undefined' && module.exports) module.exports = diagnosticsCategories;
