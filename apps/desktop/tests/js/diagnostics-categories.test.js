// Проверки предупреждения перед сбором диагностики: пользователь видит, какие
// сведения будут обезличены. Прежняя проверка сверяла замороженную константу
// сама с собой, поэтому не видела ни ключа без подписи в settings.js, ни
// расхождения с перечнем категорий ядра.
// Проверяется настоящий путь: нажатие кнопки в настройках открывает диалог, и
// пункты списка читаются из окна.
// Спецификация: specs/diagnostics-bundle.md, S-014.
// Запуск: node --test apps/desktop/tests/js/diagnostics-categories.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {DIAGNOSTICS_CATEGORY_KEYS} = require('../../ui/modules/diagnostics-categories.js');
const {createUiWindow} = require('./ui-window.js');

// Какой строкой диалога закрыта каждая категория обезличивания ядра. Ядро
// обезличивает идентификаторы по отдельности, а пользователю они понятнее
// одной строкой, поэтому несколько категорий ведут к одному ключу.
const CORE_CATEGORY_TO_KEY = {
  email: 'diagnosticsCategoryEmail',
  host: 'diagnosticsCategoryHost',
  path: 'diagnosticsCategoryPath',
  folder: 'diagnosticsCategoryFolder',
  account_id: 'diagnosticsCategoryIds',
  message_id: 'diagnosticsCategoryIds',
  folder_id: 'diagnosticsCategoryIds',
  uuid: 'diagnosticsCategoryIds',
  uid: 'diagnosticsCategoryIds',
  message_id_header: 'diagnosticsCategoryIds',
  // Содержимое письма перечнем категорий не описывается: о нём говорит
  // отдельная оговорка диалога про произвольный текст ответа сервера.
  content: null,
};

// Категории ядра читаются из его же таблицы имён: перечень в интерфейсе
// составлен по ней, и разойтись они не должны молча.
function coreCategoryTags() {
  const source = fs.readFileSync(path.join(__dirname, '..', '..', '..', '..', 'crates', 'core', 'src', 'diagnostics.rs'), 'utf8');
  const start = source.indexOf('pub const fn tag(self)');
  assert.ok(start > 0, 'в ядре не нашлась таблица имён категорий обезличивания');
  const body = source.slice(start, source.indexOf('}', source.indexOf('match self', start)));
  return [...body.matchAll(/=>\s*"([a-z_]+)"/g)].map(match => match[1]);
}

async function openDiagnosticsDialog(locale) {
  const ui = createUiWindow({locale});
  await ui.clock.drain();
  await ui.evaluate('window.localizationReady');
  ui.evaluate(`applyWizardLanguage(${JSON.stringify(locale)},false);`);
  await ui.clock.drain();
  ui.byId('collectDiagnostics').dispatch('click');
  await ui.clock.drain();
  return ui;
}

test('S-014: диалог перед сбором показывает подписанный перечень категорий на обоих языках', async () => {
  for (const locale of ['ru', 'en']) {
    const ui = await openDiagnosticsDialog(locale);
    assert.ok(ui.byId('diagOverlay').classes.has('open'),
      `язык ${locale}: предупреждение перед сбором не открылось - пользователь не узнает, что попадёт в архив`);
    const items = ui.queryAll('#diagCategories li').map(item => item.textContent.trim());
    assert.equal(items.length, DIAGNOSTICS_CATEGORY_KEYS.length,
      `язык ${locale}: в перечне показано не столько категорий, сколько их объявлено`);
    items.forEach((text, index) => {
      assert.ok(text.length > 0,
        `язык ${locale}: категория ${DIAGNOSTICS_CATEGORY_KEYS[index]} показана пустой строкой - для ключа нет подписи`);
      assert.equal(text.includes('diagnosticsCategory'), false,
        `язык ${locale}: вместо подписи категории показан её ключ`);
    });
    assert.equal(new Set(items).size, items.length, `язык ${locale}: две категории показаны одной подписью`);
    // Оговорка про произвольный текст ответа сервера - вторая половина
    // требования: без неё перечень обещает больше, чем архив выполняет.
    assert.ok(ui.byId('diagCaveat').textContent.trim().length,
      `язык ${locale}: оговорка про непокрытые сведения пуста`);
  }

  // Подписи разных языков должны отличаться: одинаковый текст означает, что
  // раздел не перевёлся вовсе.
  const russian = await openDiagnosticsDialog('ru');
  const english = await openDiagnosticsDialog('en');
  assert.notDeepEqual(
    russian.queryAll('#diagCategories li').map(item => item.textContent),
    english.queryAll('#diagCategories li').map(item => item.textContent),
    'перечень категорий не перевёлся: на обоих языках показан один и тот же текст');
});

test('S-014: каждая категория обезличивания ядра закрыта строкой перечня', () => {
  const tags = coreCategoryTags();
  assert.ok(tags.includes('email') && tags.includes('folder'), 'таблица имён категорий ядра прочитана неверно');
  tags.forEach(tag => {
    assert.ok(Object.prototype.hasOwnProperty.call(CORE_CATEGORY_TO_KEY, tag),
      `категория ядра "${tag}" не описана в перечне интерфейса: пользователь не узнает, что эти сведения тоже заменяются`);
    const key = CORE_CATEGORY_TO_KEY[tag];
    if (key === null) return;
    assert.ok(DIAGNOSTICS_CATEGORY_KEYS.includes(key),
      `категория ядра "${tag}" ведёт к ключу ${key}, которого нет в перечне интерфейса`);
  });

  // Обратная сторона: лишний ключ в перечне обещал бы обезличивание, которого
  // ядро не делает.
  const covered = new Set(Object.values(CORE_CATEGORY_TO_KEY).filter(Boolean));
  DIAGNOSTICS_CATEGORY_KEYS.forEach(key => {
    assert.ok(covered.has(key), `ключ перечня ${key} не соответствует ни одной категории обезличивания ядра`);
  });
  assert.equal(new Set(DIAGNOSTICS_CATEGORY_KEYS).size, DIAGNOSTICS_CATEGORY_KEYS.length,
    'в перечне категорий есть повтор: пользователь увидит одну строку дважды');
});
