// Проверки очереди отправки: обратный отсчёт окна отмены, раздел "Исходящие"
// и возврат отменённого письма в композер.
// Разделы проверяются настоящим путём: окно собирается из настоящего
// index.html, модули выполняются целиком, нажатия идут теми же обработчиками,
// которые вызовет браузер, а итог смотрится по тому, что уходит в ядро и что
// видит человек. Проверки чистых функций рядом с окном этого не ловили: подпись
// строки собиралась верно, пока раздел её не звал вовсе.
// Спецификация: specs/undo-send.md.
// Запуск: node --test apps/desktop/tests/js/outbox.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const outbox = require('../../ui/modules/outbox.js');
const {
  openWindow, openQueueSection, fillAccountSelect, switchLanguage,
  sectionRows, rowButton, rowLabels, shownMessages, limitPayload,
} = require('./queue-window.js');

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

// Очередь отправки в том виде, в каком её отдаёт ядро разделу "Исходящие":
// письмо с неизвестным итогом в выключенном ящике, ожидающее письмо без темы и
// уже отменённое письмо.
function queueEntries() {
  return [
    {id: 7, account_id: 3, account_enabled: false, status: 'uncertain', subject: 'Отчёт',
      to: ['boss@example.test'], attempts: 2, last_error: 'соединение оборвано'},
    {id: 8, account_id: 3, account_enabled: true, status: 'pending', subject: '   ',
      to: ['mate@example.test'], attempts: 0},
    {id: 9, account_id: 3, account_enabled: true, status: 'cancelled', subject: 'Договор',
      to: ['boss@example.test'], attempts: 0},
  ];
}

test('S-029, S-062, S-064: раздел "Исходящие" называет состояние письма и даёт по нему действия', async () => {
  const ui = openWindow({answers: {listOutboxSends: () => queueEntries()}});
  await openQueueSection(ui, 'outboxAccount');
  const rows = sectionRows(ui, 'outboxList');
  assert.equal(rows.length, 3, 'раздел не построил строки очереди: проверять нечего');

  const uncertain = rows[0].children[0].textContent;
  // Неопределённый итог прежде выдавался за отказ: человеку говорили, что
  // письмо не ушло, хотя оно могло дойти до получателя.
  assert.ok(uncertain.includes('Итог неизвестен'), uncertain);
  assert.ok(!uncertain.includes('Отправить не удалось'), 'неопределённый итог выдан за отказ');
  // Выключенный ящик объясняется прямо: иначе письмо просто висит без причины.
  assert.ok(uncertain.includes('ящик выключен'), uncertain);
  assert.ok(uncertain.includes('попыток: 2'), uncertain);
  assert.ok(uncertain.includes('соединение оборвано'), uncertain);
  // Письмо без темы показывается словами, а не пустым местом.
  assert.ok(rows[1].children[0].textContent.startsWith('без темы'), rows[1].children[0].textContent);

  // Действия строки зависят от состояния: отменять можно только то, что ещё
  // ждёт отправки, а повторять - то, что уже отказало или неизвестно чем
  // закончилось.
  assert.deepEqual(rowLabels(rows[0]), ['Повторить', 'Удалить'], 'у письма с неизвестным итогом не те действия');
  assert.deepEqual(rowLabels(rows[1]), ['Отменить', 'Удалить'], 'у ожидающего письма не те действия');
  assert.deepEqual(rowLabels(rows[2]), ['Открыть', 'Удалить'], 'у отменённого письма не те действия');

  // Смена языка идёт настоящим путём окна: раздел собран в коде, и без
  // пересборки он остался бы на прежнем языке до перезапуска программы.
  await switchLanguage(ui, 'en');
  const english = sectionRows(ui, 'outboxList')[0].children[0].textContent;
  assert.ok(english.includes('Outcome unknown'), english);
  assert.ok(english.includes('the mailbox is off'), english);
  assert.equal(ui.callsOf('listOutboxSends').length, 2,
    'смена языка заново дёрнула ядро: подписи собраны в коде, перечитывать очередь незачем');
});

test('S-038, S-044, S-064: итог отмены называет настоящее состояние операции', async () => {
  // Прежде любое состояние, кроме передачи, объявлялось отправленным письмом:
  // человеку говорили, что письмо ушло, хотя оно отказало, ждёт его решения или
  // уже отменено. Итог смотрится там, где его видит человек - в показанном
  // сообщении, а не в возврате функции.
  const outcomes = [
    {outcome: 'cancelled', says: 'отменена', avoids: 'отозвать'},
    {outcome: 'already_sending', says: 'уже началась', avoids: 'отозвать'},
    {outcome: 'already_sent', says: 'отозвать его нельзя', avoids: 'отменена'},
    {outcome: 'uncertain', says: 'неизвестен', avoids: 'отправлено'},
    {outcome: 'already_failed', says: 'не удалось', avoids: 'отправлено'},
    {outcome: 'already_cancelled', says: 'уже отменена', avoids: 'отправлено'},
  ];
  let outcome = 'cancelled';
  const ui = openWindow({answers: {
    listOutboxSends: () => queueEntries().filter(entry => entry.status === 'pending'),
    cancelSend: () => outcome,
  }});
  await openQueueSection(ui, 'outboxAccount');

  for (const item of outcomes) {
    outcome = item.outcome;
    const row = sectionRows(ui, 'outboxList')[0];
    const cancel = rowButton(row, 'Отменить');
    assert.ok(cancel, `в строке ожидающего письма нет действия "Отменить" (${item.outcome})`);
    cancel.dispatch('click');
    await ui.clock.drain();
    const shown = shownMessages(ui).at(-1) || '';
    assert.ok(shown.includes(item.says), `итог ${item.outcome} показан как "${shown}"`);
    assert.ok(!shown.includes(item.avoids), `итог ${item.outcome} обещает лишнее: "${shown}"`);
  }
  const call = ui.callsOf('cancelSend').at(-1);
  assert.deepEqual([call.args[0], call.args[1]], [3, 8], 'отмена ушла в ядро не по тому письму');

  // На английском итог переведён: иначе человек читает чужой язык в ответ на
  // своё действие.
  await switchLanguage(ui, 'en');
  outcome = 'already_sent';
  rowButton(sectionRows(ui, 'outboxList')[0], 'Cancel').dispatch('click');
  await ui.clock.drain();
  assert.ok((shownMessages(ui).at(-1) || '').includes('cannot be recalled'), shownMessages(ui).at(-1));
});

// Отменённое письмо в том виде, в каком его отдаёт ядро по openCancelledSend.
function cancelledMessage() {
  return {
    operation_id: 9, account_id: 3,
    to: ['boss@example.test'], cc: [], bcc: ['secret@example.test'],
    subject: 'Договор', body_html: '<p>текст</p>', body_text: 'текст',
    attachments: [{filename: 'dogovor.pdf', mime_type: 'application/pdf', data: [1, 2, 3]}],
  };
}

test('S-040, S-041, S-042: отменённое письмо открывается из раздела и не затирает чужой черновик', async () => {
  const ui = openWindow({answers: {
    listOutboxSends: () => queueEntries().filter(entry => entry.status === 'cancelled'),
    openCancelledSend: () => cancelledMessage(),
  }});
  fillAccountSelect(ui);
  await openQueueSection(ui, 'outboxAccount');

  // S-041: в композере набирают другое письмо. Отменённое не имеет права
  // затереть его - несохранённый текст пропал бы без следа.
  ui.byId('compSubj').value = 'Другое письмо';
  rowButton(sectionRows(ui, 'outboxList')[0], 'Открыть').dispatch('click');
  await ui.clock.drain();
  assert.equal(ui.callsOf('openCancelledSend').length, 0,
    'занятый композер затирается отменённым письмом: набранное письмо пропадает');
  assert.equal(ui.callsOf('deleteSend').length, 0,
    'операция очереди удалена, хотя письмо в композер не попало: другой копии у программы нет');
  assert.equal(ui.byId('compSubj').value, 'Другое письмо', 'набранное письмо затёрто');
  assert.ok((shownMessages(ui).at(-1) || '').includes('Исходящие'),
    'человеку не сказали, где ждёт отменённое письмо');

  // Композер освобождают тем же способом, что и человек - кнопкой композера.
  ui.byId('compDeleteDraft').dispatch('click');
  await ui.clock.drain();

  rowButton(sectionRows(ui, 'outboxList')[0], 'Открыть').dispatch('click');
  await ui.clock.drain();
  assert.deepEqual(ui.evaluate('recipientFieldAddresses("compTo")').join(','), 'boss@example.test',
    'адресаты отменённого письма не вернулись в композер');
  assert.deepEqual(ui.evaluate('recipientFieldAddresses("compBcc")').join(','), 'secret@example.test',
    'скрытая копия потерялась при возврате: получатель, которого человек не видит, выпал из письма');
  assert.equal(ui.query('[data-recipient-field="compBcc"]').classes.has('hidden'), false,
    'поле скрытой копии осталось закрытым: возвращённый адрес не виден человеку');
  assert.equal(ui.byId('compSubj').value, 'Договор');
  assert.equal(ui.evaluate('composerAttachments.length'), 1, 'вложение не вернулось в композер');
  assert.ok(ui.byId('compAtt').textContent.includes('dogovor.pdf'), 'вложение не показано в письме');

  // S-003, S-042: операция очереди - единственная долговечная копия письма.
  // Её удаление раньше подтверждённой записи черновика теряло письмо целиком.
  const order = ui.calls.map(call => call.command);
  const draftSaved = order.lastIndexOf('setSetting');
  const removed = order.lastIndexOf('deleteSend');
  assert.ok(removed > draftSaved && draftSaved >= 0,
    'операция очереди удалена раньше записи черновика: отказ записи потерял бы письмо');
  assert.deepEqual(ui.callsOf('deleteSend').at(-1).args, [3, 9], 'из очереди убрано не то письмо');
});

test('S-012: пустое поле длительности окна отмены не выключает отмену молча', async () => {
  // Проверяется настоящий путь поля: значение приводилось к числу до проверки,
  // пустая строка становилась нулём, проходила как допустимая и молча выключала
  // окно отмены.
  // Границы приходят из ядра. Числа здесь нарочно не те, что у ядра по
  // умолчанию: прежде одна и та же граница жила тремя копиями - в ядре, в
  // модуле и в атрибутах min/max разметки.
  const saved = [];
  const ui = openWindow({answers: {
    limitSettings: () => limitPayload({limit_undo_send_min: 0, limit_undo_send_max: 30}),
    undoSendSeconds: () => 5,
    setUndoSendSeconds: value => { saved.push(value); return value; },
  }});
  await ui.evaluate('reloadLimitSettings()');
  await ui.evaluate('loadUndoSendSetting()');
  await ui.clock.drain();
  const field = ui.byId('undoSendSeconds');
  assert.equal(field.value, '5', 'поле не показало действующее значение ядра');
  assert.equal(field.max, '30', 'верхняя граница поля взята не из ответа ядра');

  field.value = '';
  field.dispatch('change');
  await ui.clock.drain();
  assert.deepEqual(saved, [], 'очищенное поле ушло в ядро и выключило окно отмены');
  assert.equal(field.value, '5', 'поле не вернулось к действующему значению');
  assert.ok((shownMessages(ui).at(-1) || '').includes('от 0 до 30'),
    'отказ не назвал границы ядра: прежде в тексте стояли свои числа');

  field.value = '7';
  field.dispatch('change');
  await ui.clock.drain();
  assert.deepEqual(saved, [7], 'допустимое значение не сохранилось');

  field.value = '30';
  field.dispatch('change');
  await ui.clock.drain();
  assert.deepEqual(saved, [7, 30], 'значение на верхней границе не сохранилось');

  field.value = '31';
  field.dispatch('change');
  await ui.clock.drain();
  assert.deepEqual(saved, [7, 30], 'значение вне границ ядра ушло в ядро');
});
