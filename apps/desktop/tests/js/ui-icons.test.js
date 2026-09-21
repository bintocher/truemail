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
const read = file => fs.readFileSync(path.join(uiRoot, file), 'utf8');
const registry = read('modules/shell.js');

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

// Имя значка и место, где оно записано, - для внятного сообщения о пропаже.
function everyFile(collect) {
  const found = [];
  for (const file of uiFiles()) {
    const where = path.relative(uiRoot, file).replace(/\\/g, '/');
    for (const name of collect(fs.readFileSync(file, 'utf8'))) found.push({name, where});
  }
  return found;
}

const matches = (text, pattern, group = 1) => [...text.matchAll(pattern)].map(match => match[group]);

test('значок, записанный в разметке именем, есть в реестре', () => {
  const names = iconNames();
  assert.ok(names.size > 20, 'реестр значков не разобрался');
  const missing = everyFile(text => matches(text, /data-i="([^"${}]+)"/g))
    .filter(item => !names.has(item.name))
    .map(item => `${item.where}: ${item.name}`);
  assert.deepEqual(missing, [], `значка нет в реестре, на его месте пусто:\n${missing.join('\n')}`);
});

// Значки теряются не в разметке, а там, где имя подставляется кодом:
// data-i="${icon}" рисует пустоту так же молча, а имя приходит из перечней и
// таблиц действий. Каждый такой перечень проверяется отдельно, и у каждого
// задан нижний предел числа имён: если разметка источника изменится и разбор
// его перестанет видеть, проверка это покажет, а не промолчит.
test('значок, подставляемый кодом, есть в реестре', () => {
  const names = iconNames();
  const quickSteps = read('modules/quick-steps.js');
  const toolbar = /const tbActions=\[([\s\S]*?)\];/.exec(read('modules/smart-rules.js'));
  const contextMenu = read('modules/calendar-contacts.js')
    .split('\n')
    .filter(line => line.includes('data-i="${'))
    .join('\n');
  const sources = [
    {
      name: 'перечень значков быстрого действия и умной папки (quick-steps.js, ICONS)',
      least: 30,
      icons: matches(/const ICONS = \[([\s\S]*?)\];/.exec(quickSteps)?.[1] || '', /'([a-z][\w-]*)'/g),
      why: 'имя из этого перечня подставляется в разметку окна как есть',
    },
    {
      name: 'значки действий панели письма (smart-rules.js, tbActions)',
      least: 8,
      // Значок действия - поле i, а если его нет, то сам ключ действия.
      icons: matches(toolbar?.[1] || '', /\{([^{}]*)\}/g)
        .map(body => /[,{]i:'([^']+)'/.exec(body)?.[1] || /k:'([^']+)'/.exec(body)?.[1])
        .filter(Boolean),
      why: 'действие без своего значка берёт значок по ключу, и опечатка в ключе оставляет кнопку пустой',
    },
    {
      name: 'значки пунктов меню письма (calendar-contacts.js)',
      least: 8,
      icons: matches(contextMenu, /\[\s*'[a-z][\w-]*'\s*,\s*'([a-z][\w-]*)'\s*,\s*'/g),
      why: 'пункты меню перечислены тройками "действие, значок, подпись"',
    },
    {
      name: 'значки в полях i объектов интерфейса',
      least: 15,
      icons: everyFile(text => matches(text, /[{,]\s*i\s*:\s*'([A-Za-z][\w-]*)'/g)).map(item => item.name),
      why: 'поле i попадает прямо в data-i при отрисовке команд, правил и быстрых действий',
    },
  ];
  const complaints = [];
  for (const source of sources) {
    const icons = [...new Set(source.icons)];
    if (icons.length < source.least) {
      complaints.push(`${source.name}: разобрано имён ${icons.length}, ожидалось не меньше ${source.least} - источник перестал читаться`);
      continue;
    }
    for (const icon of icons) {
      if (!names.has(icon)) complaints.push(`${source.name}: значка "${icon}" нет в реестре (${source.why})`);
    }
  }
  assert.deepEqual(complaints, [], `подставляемого значка нет в реестре, на его месте пусто:\n${complaints.join('\n')}`);
});

// Значок, взятый из реестра по имени (ic.trash, ic['star']), при опечатке даёт
// undefined: кнопка получает пустую разметку вместо картинки, и снова без
// единой ошибки.
test('значок, взятый из реестра по имени, в реестре есть', () => {
  const names = iconNames();
  const used = everyFile(text => [
    ...matches(text, /\bic\.([A-Za-z][\w]*)/g),
    ...matches(text, /\bic\[\s*'([^']+)'\s*\]/g),
  ]);
  assert.ok(new Set(used.map(item => item.name)).size >= 4, 'обращения к реестру перестали разбираться');
  const missing = used.filter(item => !names.has(item.name)).map(item => `${item.where}: ic.${item.name}`);
  assert.deepEqual(missing, [], `обращение к несуществующему значку даёт undefined:\n${missing.join('\n')}`);
});
