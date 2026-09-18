// Проверки чистой логики автоответа "нет на месте": объяснение выбранного
// режима, границы периода и разбор перечня внутренних доменов.
// Спецификация: specs/out-of-office.md.
// Запуск: node --test apps/desktop/tests/js/out-of-office.test.js (Node 22+).

const test = require('node:test');
const assert = require('node:assert/strict');
const oof = require('../../ui/modules/out-of-office.js');

test('S-004, S-007, S-008, S-020: режим объясняется честно и не обещает лишнего', () => {
  const server = oof.oofModeExplanation({mode: 'server', available: true}, 'ru');
  assert.ok(server.includes('на сервере'), server);
  assert.ok(server.includes('определяет сервер'), server);
  const local = oof.oofModeExplanation({mode: 'local', available: true}, 'ru');
  assert.ok(local.includes('пока программа запущена'), local);
  assert.ok(!local.includes('при закрытой программе'), 'локальный режим не обещает работу при закрытой программе');
  // Ящик Exchange вне сборки Windows: локальный автоответ ему не предлагается.
  const unavailable = oof.oofModeExplanation(
    {mode: 'server', available: false, unavailable_reason: 'автоответ ящика Exchange доступен только в сборке для Windows'},
    'ru',
  );
  assert.equal(unavailable, 'автоответ ящика Exchange доступен только в сборке для Windows');
  assert.ok(!unavailable.includes('пока программа запущена'));
});

test('S-021, S-023, S-026: границы периода, текстов и перечня доменов проверяются до обращения к ядру', () => {
  const base = {
    enabled: true,
    starts_at: '2026-10-01T00:00:00.000Z',
    ends_at: '2026-10-10T00:00:00.000Z',
    internal_text: 'Я в отпуске',
    external_text: 'Out of office',
    internal_domains: ['example.test'],
  };
  assert.equal(oof.oofValidationError(base, 'ru'), null);
  // Ровно минута - нижняя граница, она допустима.
  assert.equal(oof.oofValidationError({...base, ends_at: '2026-10-01T00:01:00.000Z'}, 'ru'), null);
  assert.ok(oof.oofValidationError({...base, ends_at: '2026-10-01T00:00:30.000Z'}, 'ru'));
  assert.ok(oof.oofValidationError({...base, ends_at: '2027-11-01T00:00:00.000Z'}, 'ru'));
  assert.ok(oof.oofValidationError({...base, internal_text: '   '}, 'ru'));
  assert.ok(oof.oofValidationError({...base, external_text: 'x'.repeat(10001)}, 'ru'));
  assert.ok(oof.oofValidationError({...base, internal_domains: []}, 'ru'));
  assert.ok(oof.oofValidationError({...base, internal_domains: Array.from({length: 21}, (_, i) => `d${i}.test`)}, 'ru'));
  // Домен без точки накрыл бы целую доменную зону.
  assert.ok(oof.oofValidationError({...base, internal_domains: ['localhost']}, 'ru'));
  // Выключенный автоответ ничего не требует.
  assert.equal(oof.oofValidationError({...base, enabled: false, internal_text: ''}, 'ru'), null);
});

test('S-013, S-027: домены приводятся к общей форме, а период уходит во всемирном времени', () => {
  assert.deepEqual(
    oof.oofDomainsFromText(' @Example.TEST, sub.example.test.\n example.test '),
    ['example.test', 'sub.example.test'],
    'знак @, завершающая точка, регистр и дубли снимаются',
  );
  const input = oof.oofInput({
    accountId: '7',
    enabled: true,
    startsAt: '2026-10-01T03:00',
    endsAt: '2026-10-10T03:00',
    internalText: '  Я в отпуске  ',
    externalText: 'Out of office',
    domains: 'example.test',
  });
  assert.equal(input.account_id, 7);
  assert.ok(input.starts_at.endsWith('Z'), 'период уходит во всемирном времени');
  assert.equal(input.internal_text, 'Я в отпуске');
  assert.deepEqual(input.internal_domains, ['example.test']);
});
