// Пределы для проверок интерфейса.
// Модули интерфейса берут числа из реестра пределов, который в рабочем пути
// заполняет ядро (команда limit_settings). В проверках его заполняет эта
// заготовка, и значения она берёт нарочно не такие, как у ядра: тогда
// сработавший предел доказывает, что модуль прочитал реестр, а не своё число.
// См. apps/desktop/ui/modules/limits.js и crates/core/src/model/limits.rs.
'use strict';
const limits = require('../../ui/modules/limits.js');

// Описание поля в том виде, в каком его отдаёт ядро.
function spec(key, value) {
  return {
    key,
    section: 'test',
    title_key: `title:${key}`,
    hint_key: `hint:${key}`,
    unit_key: `unit:${key}`,
    default: value,
    min: 0,
    max: 1000000,
    value,
  };
}

// Применить набор пределов. Ключи - те же, что у ядра (limits.KEYS).
function applyTestLimits(values) {
  limits.applyLimits({
    sections: [{id: 'test', title_key: 'section:test', hint_key: 'hint:test'}],
    limits: Object.entries(values).map(([key, value]) => spec(key, value)),
  });
  return limits;
}

// Очистить реестр: так проверяется путь до загрузки пределов, когда решение
// остаётся за ядром.
function clearTestLimits() {
  limits.applyLimits({sections: [], limits: []});
  return limits;
}

module.exports = {limits, applyTestLimits, clearTestLimits};
