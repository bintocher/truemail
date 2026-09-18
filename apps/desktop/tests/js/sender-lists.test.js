// Проверки чистой логики списков отправителей, игнорируемых переписок и
// автоочистки по отправителю.
// Спецификации: specs/blocked-senders.md, specs/ignore-conversation.md,
// specs/sweep-by-sender.md.
// Запуск: node --test apps/desktop/tests/js/sender-lists.test.js (Node 22+).

const test = require('node:test');
const assert = require('node:assert/strict');
const senders = require('../../ui/modules/sender-lists.js');

test('S-027, S-028: диалог блокировки берёт адрес письма и его домен, а непригодное значение отвергает', () => {
  // Адрес приходит и разобранным полем, и строкой с отображаемым именем:
  // отображаемое имя в значение записи попасть не должно.
  const parsed = senders.senderChoice({from: {email: 'Boss@Example.Test'}});
  assert.equal(parsed.address, 'Boss@Example.Test');
  assert.equal(parsed.domain, 'example.test');
  assert.equal(parsed.kind, 'address', 'по умолчанию предлагается адрес');
  const named = senders.senderChoice({from_addr: 'Иван Петров <ivan@sub.example.test.>'});
  assert.equal(named.address, 'ivan@sub.example.test.');
  assert.equal(named.domain, 'sub.example.test');
  // Домен без точки записью не становится: такая запись накрыла бы целую
  // доменную зону.
  assert.equal(senders.senderChoice({from: {email: 'root@localhost'}}), null);
  assert.equal(senders.senderChoice({from: {email: ''}}), null);
  assert.equal(senders.senderChoice(null), null);
});

test('S-032, S-046: состав уборки уходит в ядро с нормализованными полями и проверенным числом дней', () => {
  const choice = senders.senderChoice({from: {email: 'shop@example.test'}});
  const once = senders.sweepInput(choice, {mode: 'once', accountId: 4, days: '30', sweepArchive: false});
  assert.deepEqual(once, {
    address: 'shop@example.test',
    mode: 'once',
    account_id: 4,
    days: null,
    sweep_archive: false,
  });
  const older = senders.sweepInput(choice, {mode: 'older_than', accountId: null, days: '90', sweepArchive: true});
  assert.equal(older.days, 90, 'число дней уходит числом, а не строкой');
  assert.equal(older.account_id, null, 'пустая область означает все ящики');
  assert.equal(older.sweep_archive, true, 'архив берётся только по согласию');
  assert.ok(senders.validSweepDays(1) && senders.validSweepDays(3650));
  assert.ok(!senders.validSweepDays(0) && !senders.validSweepDays(3651));
  assert.ok(!senders.validSweepDays('30.5') && !senders.validSweepDays(''));
});

test('S-034, S-038, S-048, S-051: отчёты и подписи честны и переключаются вместе с языком', () => {
  const report = {queued: 3, skipped: 2, failed: 1, irreversible: 4, remaining: 5};
  const ru = senders.returnReportText(report, 'ru');
  const en = senders.returnReportText(report, 'en');
  // Возврат называется возвращением опознанных писем, а число ненайденных
  // показывается пользователю, а не замалчивается.
  assert.match(ru, /не найдено в корзине 2/);
  assert.match(en, /not found in trash 2/);
  assert.match(senders.sweepReportText(report, 'ru'), /осталось 5/);
  assert.match(senders.sweepReportText(report, 'en'), /left 5/);
  assert.notEqual(
    senders.sweepModeText('older_than', 90, 'ru'),
    senders.sweepModeText('older_than', 90, 'en'),
  );
  assert.match(senders.sweepModeText('older_than', 90, 'ru'), /90/);
  assert.notEqual(senders.ignoreStateText('return_failed', 'ru'), senders.ignoreStateText('return_failed', 'en'));
  assert.notEqual(senders.policyDecisionText('trusted', 'ru'), senders.policyDecisionText('trusted', 'en'));
  // Запись списка показывает вид, значение, решение и число убранных писем.
  const row = senders.policyRowText({kind: 'domain', value: 'spam.test', decision: 'blocked', swept: 7}, 'ru');
  assert.match(row, /spam\.test/);
  assert.match(row, /7/);
  assert.equal(senders.sweepScopeText({account_id: null}, [], 'en'), 'All mailboxes');
  assert.equal(
    senders.sweepScopeText({account_id: 2}, [{id: 2, email: 'me@example.test'}], 'ru'),
    'me@example.test',
  );
});
