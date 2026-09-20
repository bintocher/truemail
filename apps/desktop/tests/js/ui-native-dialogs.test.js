// Проверка того, что интерфейс не спрашивает подтверждения нативным окном
// браузера. Плагин диалогов Tauri подменяет window.confirm своей обёрткой, а
// команды, которую та вызывает, в плагине больше нет: в приложении нативный
// confirm никогда не показывает окна и всегда отвечает отказом. Из-за этого
// снятие флажка у письма со сроками молча не доходило до ядра, а проверки
// вида if(!confirm(...))return выполняли удаление вовсе без вопроса.
// Спецификация: specs/flag-due-dates.md (S-002, S-024).
// Запуск: node --test apps/desktop/tests/js/ui-native-dialogs.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const uiRoot = path.join(__dirname, '../../ui');

function uiScripts(directory = uiRoot) {
  const found = [];
  for (const entry of fs.readdirSync(directory, {withFileTypes: true})) {
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) found.push(...uiScripts(full));
    else if (entry.name.endsWith('.js')) found.push(full);
  }
  return found;
}

// Вызов нативного окна: имя confirm не через точку (собственная обёртка
// confirmAction под шаблон не подходит) либо явное обращение к window.
const NATIVE_CONFIRM = /(^|[^\w.$])confirm\s*\(|\b(?:window|globalThis)\.confirm\s*\(/;

test('S-002, S-024: подтверждения идут своим окном, а не нативным confirm', () => {
  const guilty = [];
  for (const file of uiScripts()) {
    const text = fs.readFileSync(file, 'utf8');
    text.split('\n').forEach((line, index) => {
      if (NATIVE_CONFIRM.test(line)) {
        guilty.push(`${path.relative(uiRoot, file).replace(/\\/g, '/')}:${index + 1}`);
      }
    });
  }
  assert.deepEqual(guilty, [],
    'нативный confirm в приложении всегда отвечает отказом: подтверждение обязано идти через confirmAction, '
    + `иначе действие молча не выполняется или выполняется без вопроса. Найдено: ${guilty.join(', ')}`);
});
