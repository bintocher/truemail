// Проверки чистой логики очереди отправки: обратный отсчёт окна отмены,
// подписи раздела "Исходящие" и возврат отменённого письма в композер.
// Спецификация: specs/undo-send.md.
// Запуск: node --test apps/desktop/tests/js/outbox.test.js (Node 22+).

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const outbox = require('../../ui/modules/outbox.js');

// Раздел настроек живёт в queue-sections.js и работает с DOM. Чтобы проверять
// рабочий путь, а не отдельную функцию рядом с ним, модуль выполняется целиком
// в подставном окружении, и дальше вызывается тот же слушатель поля, который
// вызовет браузер.
function loadQueueSections(undoField, tm) {
  const listeners = new Map();
  const toasts = [];
  const nodes = {undoSendSeconds: undoField};
  const context = {
    console,
    wizardLocale: 'ru',
    coreAccounts: [],
    L: russian => russian,
    showToast: value => toasts.push(String(value)),
    confirmAction: async () => true,
    document: {
      getElementById: id => {
        const node = nodes[id];
        if (!node) return null;
        node.addEventListener = (type, handler) => listeners.set(`${id}:${type}`, handler);
        return node;
      },
      querySelectorAll: () => [],
    },
  };
  context.window = context;
  context.window.tm = tm;
  context.window.outboxModel = outbox;
  vm.createContext(context);
  vm.runInContext(
    fs.readFileSync(path.join(__dirname, '..', '..', 'ui', 'modules', 'queue-sections.js'), 'utf8'),
    context,
  );
  return {listeners, toasts};
}

test('S-017, S-018: обратный отсчёт считает срок отмены во всемирном времени', () => {
  // Время очереди приходит из базы без обозначения зоны. Прочитанное как
  // местное, оно дало бы ошибку на целое смещение часового пояса: в Москве
  // карточка висела бы три лишних часа, а западнее исчезала бы сразу.
  const now = Date.parse('2026-09-18T12:00:00Z');
  assert.equal(outbox.remainingUndoSeconds('2026-09-18 12:00:05', now), 5);
  assert.equal(outbox.remainingUndoSeconds('2026-09-18T12:00:05+00:00', now), 5);
  // Срок уже наступил - действия "Отменить" быть не должно.
  assert.equal(outbox.remainingUndoSeconds('2026-09-18 11:59:59', now), 0);
  assert.equal(outbox.remainingUndoSeconds('', now), 0);
});

test('S-019, S-058, S-059: действие "Отменить" показывается только обычной отправке с ненулевым окном', () => {
  const now = Date.parse('2026-09-18T12:00:00Z');
  const queued = {operation_id: 1, account_id: 2, cancel_until: '2026-09-18 12:00:05', origin: 'ordinary'};
  assert.ok(outbox.showsUndoAction(queued, now));
  // Нулевое окно: письмо доступно работнику сразу, отменять нечего.
  assert.ok(!outbox.showsUndoAction({...queued, cancel_until: '2026-09-18 12:00:00'}, now));
  // Служебное письмо и письмо внешней программы карточки не получают.
  assert.ok(!outbox.showsUndoAction({...queued, origin: 'automatic'}, now));
  assert.ok(!outbox.showsUndoAction({...queued, origin: 'external'}, now));
  // Повторное нажатие вернуло прежнюю операцию - второй карточки быть не должно.
  assert.ok(!outbox.showsUndoAction({...queued, duplicate: true}, now));
});

test('S-029, S-064: строка раздела не выдаёт неопределённый итог за отказ и объясняет выключенный ящик', () => {
  const entry = {
    id: 7, account_id: 3, account_enabled: false, status: 'uncertain',
    subject: 'Отчёт', to: ['boss@example.test'], attempts: 2, last_error: 'соединение оборвано',
  };
  const ru = outbox.outboxRowText(entry, 'ru');
  assert.ok(ru.includes('Итог неизвестен'), ru);
  assert.ok(!ru.includes('Отправить не удалось'), 'неопределённый итог не выдаётся за отказ');
  assert.ok(ru.includes('ящик выключен'), ru);
  assert.ok(ru.includes('попыток: 2'), ru);
  const en = outbox.outboxRowText(entry, 'en');
  assert.ok(en.includes('Outcome unknown'), en);
  assert.ok(en.includes('the mailbox is off'), en);
  // Письмо без темы не показывается пустой строкой.
  assert.ok(outbox.outboxRowText({...entry, subject: '  '}, 'ru').startsWith('без темы'));
});

test('S-038, S-044: отказ отмены называет причину и не обещает отзыв у получателей', () => {
  assert.ok(outbox.cancelOutcomeText('already_sending', 'ru').includes('уже началась'));
  const sent = outbox.cancelOutcomeText('already_sent', 'ru');
  assert.ok(sent.includes('отозвать его нельзя'), sent);
  assert.ok(outbox.cancelOutcomeText('cancelled', 'en').includes('cancelled'));
});

test('S-040: отменённое письмо возвращается в композер целиком, вместе с вложениями', () => {
  const draft = outbox.composerDraftFromCancelled({
    operation_id: 12,
    account_id: 4,
    to: ['a@example.test'],
    cc: ['b@example.test'],
    bcc: ['secret@example.test'],
    subject: 'Договор',
    body_html: '<p>текст</p>',
    body_text: 'текст',
    attachments: [{filename: 'договор.pdf', mime_type: 'application/pdf', data: new Uint8Array([1, 2, 3])}],
  });
  assert.deepEqual(draft.bcc, ['secret@example.test'], 'скрытые копии возвращаются вместе с письмом');
  assert.equal(draft.attachments.length, 1);
  assert.deepEqual(draft.attachments[0].data, [1, 2, 3], 'байты вложения приходят перечнем чисел');
  assert.equal(draft.operation_id, 12);
});

test('S-064: отмена называет настоящее состояние операции', () => {
  // Прежде любое состояние, кроме передачи, объявлялось отправленным письмом:
  // человеку говорили, что письмо ушло, хотя оно отказало или ждёт его решения.
  assert.ok(outbox.cancelOutcomeText('uncertain', 'ru').includes('неизвестен'));
  assert.ok(!outbox.cancelOutcomeText('uncertain', 'ru').includes('отправлено'));
  assert.ok(outbox.cancelOutcomeText('already_failed', 'ru').includes('не удалось'));
  assert.ok(outbox.cancelOutcomeText('already_cancelled', 'ru').includes('уже отменена'));
});

test('S-012: пустое поле длительности окна отмены не выключает отмену молча', async () => {
  // Проверяется сам обработчик поля: прежде он приводил значение к числу до
  // проверки, пустая строка становилась нулём, проходила как допустимая и
  // молча выключала окно отмены.
  const saved = [];
  const field = {value: ''};
  const {listeners, toasts} = loadQueueSections(field, {
    undoSendSeconds: async () => 5,
    setUndoSendSeconds: async value => {
      saved.push(value);
      return value;
    },
  });
  const change = listeners.get('undoSendSeconds:change');
  assert.ok(change, 'обработчик поля не подключён');

  await change({target: field});
  assert.deepEqual(saved, [], 'очищенное поле в ядро не уходит');
  assert.equal(field.value, '5', 'поле возвращается к действующему значению');
  assert.equal(toasts.length, 1, 'отказ объясняется человеку');

  field.value = '7';
  await change({target: field});
  assert.deepEqual(saved, [7], 'допустимое значение сохраняется');

  field.value = '61';
  await change({target: field});
  assert.deepEqual(saved, [7], 'значение вне границ в ядро не уходит');
});
