// Проверка того, что интерфейс не разговаривает с пользователем нативными
// окнами встроенного браузера. Плагин диалогов Tauri подменяет window.confirm
// и window.alert своими обёртками, а команды, которую те вызывают, в плагине
// больше нет: нативный confirm никогда не показывает окна и всегда отвечает
// отказом. Из-за этого снятие флажка у письма со сроками молча не доходило до
// ядра, а проверки вида if(!confirm(...))return выполняли удаление вовсе без
// вопроса. Нативный prompt встроенный браузер не показывает сам по себе, и
// действие обрывается без объяснения.
// Спецификация: specs/flag-due-dates.md (S-002, S-024).
// Запуск: node --test apps/desktop/tests/js/ui-native-dialogs.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const uiRoot = path.join(__dirname, '../../ui');

function uiFiles(directory = uiRoot) {
  const found = [];
  for (const entry of fs.readdirSync(directory, {withFileTypes: true})) {
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) found.push(...uiFiles(full));
    else if (/\.(js|html)$/.test(entry.name)) found.push(full);
  }
  return found;
}

// Пробелы вместо содержимого: номера строк и колонки не съезжают, поэтому
// адрес нарушения остаётся верным.
const blank = fragment => fragment.replace(/[^\n]/g, ' ');

// Примечание - не код. Пока примечания не снимались, разбор спотыкался о
// пример вида "проверки if(!confirm(...))return" в шапке файла и обвинял
// исправный модуль.
function withoutComments(text) {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, blank)
    .replace(/(^|[^:\\])\/\/[^\n]*/g, (whole, before) => before + blank(whole.slice(before.length)));
}

// Строковые значения убираем вторым шагом: текст подсказки со словом confirm
// вызовом не является. Косвенные обращения ищутся до этого шага - там имя
// диалога само записано строкой.
function withoutStrings(text) {
  return text
    .replace(/`(?:\\.|[^`\\])*`/g, blank)
    .replace(/'(?:\\.|[^'\\\n])*'/g, blank)
    .replace(/"(?:\\.|[^"\\\n])*"/g, blank);
}

// Встроенный в разметку код читается наравне с модулями: запрет, который
// обходится переносом вызова в тег script внутри html, ничего не стоит.
function scriptText(file) {
  const text = fs.readFileSync(file, 'utf8');
  if (!file.endsWith('.html')) return text;
  let inline = '';
  for (const match of text.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)) {
    if (!/\bsrc\s*=/.test(match[1])) inline += `${blank(match[0].slice(0, match[0].indexOf(match[2])))}${match[2]}\n`;
  }
  return inline;
}

// Способы дозваться до нативного окна. Прямой вызов - не единственный: имя
// берут через window по строке или вынимают из window в переменную, и проверка
// по одному только "confirm(" такой обход не видит.
function callForms(name) {
  return [
    {kind: 'прямой вызов', onCode: true, re: new RegExp(`(^|[^\\w.$])${name}\\s*\\(`)},
    {kind: 'через window', onCode: true, re: new RegExp(`\\b(?:window|globalThis|self)\\s*\\.\\s*${name}\\b`)},
    {kind: 'через window по имени', onCode: false, re: new RegExp(`\\b(?:window|globalThis|self)\\s*\\[\\s*['"\`]${name}['"\`]\\s*\\]`)},
    {kind: 'вынут из window', onCode: false, re: new RegExp(`\\{[^}\\n]*\\b${name}\\b[^}\\n]*\\}\\s*=\\s*(?:window|globalThis|self)\\b`)},
  ];
}

// Места обращения к нативному окну: [{file, line, kind}].
function nativeCalls(name) {
  const found = [];
  for (const file of uiFiles()) {
    const source = withoutComments(scriptText(file));
    const code = withoutStrings(source);
    const withText = source.split('\n');
    const withoutText = code.split('\n');
    withoutText.forEach((line, index) => {
      for (const form of callForms(name)) {
        const target = form.onCode ? line : withText[index];
        if (form.re.test(target)) {
          found.push({
            file: path.relative(uiRoot, file).replace(/\\/g, '/'),
            line: index + 1,
            kind: form.kind,
          });
          return;
        }
      }
    });
  }
  return found;
}

test('S-002, S-024: подтверждения и сообщения идут своим окном, а не нативным', () => {
  const guilty = [];
  for (const name of ['confirm', 'alert']) {
    for (const call of nativeCalls(name)) guilty.push(`${name} (${call.kind}) в ${call.file}:${call.line}`);
  }
  assert.deepEqual(guilty, [],
    'нативные confirm и alert подменены плагином диалогов и окна не показывают: confirm всегда отвечает '
    + 'отказом, поэтому действие молча не выполняется или выполняется без вопроса. Спрашивать нужно своим '
    + `окном (confirmAction), сообщать - showToast. Найдено:\n${guilty.join('\n')}`);
});

// Нативный prompt в интерфейсе ещё остался - это накопленный долг, и перечень
// ниже держит его в известных границах. Проверка падает, как только prompt
// заводится в новом файле или добавляется в старом: новых мест, где ввод
// обрывается без объяснения, быть не должно.
const PROMPT_DEBT = {
  'modules/calendar-contacts.js': 2,
  'modules/composer.js': 1,
  'modules/queue-sections.js': 2,
  'modules/sender-actions.js': 1,
  'modules/settings.js': 1,
};

test('S-002, S-024: нативный prompt не заводится в новых местах', () => {
  const byFile = new Map();
  for (const call of nativeCalls('prompt')) byFile.set(call.file, (byFile.get(call.file) || 0) + 1);
  // Долг известен и не нулевой: если разбор источников ослепнет, счётчики
  // обнулятся и проверка это покажет, а не промолчит.
  const complaints = [];
  for (const [file, count] of byFile) {
    const allowed = PROMPT_DEBT[file] || 0;
    if (count > allowed) {
      complaints.push(`${file}: обращений к нативному prompt ${count}, допущено ${allowed}`);
    }
  }
  for (const [file, allowed] of Object.entries(PROMPT_DEBT)) {
    const count = byFile.get(file) || 0;
    if (count < allowed) {
      complaints.push(`${file}: долг сократился до ${count} - уменьшите число в перечне, иначе он прикроет новый вызов`);
    }
  }
  assert.deepEqual(complaints, [],
    'нативный prompt встроенный браузер не показывает: ввод обрывается без окна и без объяснения. '
    + `Спрашивайте значение своим окном. Найдено:\n${complaints.join('\n')}`);
});
