// Проверка того, что каждый значок, на который ссылается интерфейс, есть в
// реестре значков. Отрисовка пропускает неизвестное имя молча, поэтому
// опечатка или забытый значок оставляют пустое место без единой ошибки: так
// раздел "Автоответ" и три кнопки сохранения долго стояли без значков.
// Задача: issue #106.
// Запуск: node --test apps/desktop/tests/js/ui-icons.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const uiRoot = path.join(__dirname, '../../ui');
const registry = fs.readFileSync(path.join(uiRoot, 'modules/shell.js'), 'utf8');

// Имена значков в реестре: "имя:S('<path .../>')".
function iconNames() {
  return new Set(
    [...registry.matchAll(/(?:^|[{,\s])([A-Za-z][\w-]*)\s*:\s*S\(/g)].map(match => match[1]),
  );
}

function uiFiles(directory = uiRoot) {
  const found = [];
  for (const entry of fs.readdirSync(directory, {withFileTypes: true})) {
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) found.push(...uiFiles(full));
    else if (/\.(html|js)$/.test(entry.name)) found.push(full);
  }
  return found;
}

test('каждый значок разметки есть в реестре значков', () => {
  const names = iconNames();
  assert.ok(names.size > 20, 'реестр значков не разобрался');
  const missing = [];
  for (const file of uiFiles()) {
    const source = fs.readFileSync(file, 'utf8');
    // Имена с подстановкой (data-i="${icon}") пропускаем: они известны только
    // во время работы, и проверять их чтением файла нечем.
    for (const match of source.matchAll(/data-i="([^"${}]+)"/g)) {
      if (!names.has(match[1])) missing.push(`${path.relative(uiRoot, file)}: ${match[1]}`);
    }
  }
  assert.deepEqual(missing, [], `значка нет в реестре, на его месте пусто:\n${missing.join('\n')}`);
});

// Имена, которые подставляются в разметку кодом (data-i="${icon}"), чтением
// файла не проверяются: они известны только во время работы.
