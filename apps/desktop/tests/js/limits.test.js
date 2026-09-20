// Проверки реестра пределов интерфейса: числа приходят из ядра, значение вне
// границ отклоняется с объяснением, принятое значение берётся из ответа ядра.
// Раздел настроек строится по этому же перечню.
// См. apps/desktop/ui/modules/limits.js и crates/core/src/model/limits.rs.
// Запуск: node --test apps/desktop/tests/js/limits.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const limits = require('../../ui/modules/limits.js');

// Ответ ядра в том виде, в каком его отдаёт команда limit_settings.
function corePayload() {
  return {
    sections: [
      {id: 'messages', title_key: 'limitSectionMessages', hint_key: 'limitSectionMessagesDesc'},
      {id: 'sending', title_key: 'limitSectionSending', hint_key: 'limitSectionSendingDesc'},
    ],
    limits: [
      {
        key: limits.KEYS.pinnedPerAccount, section: 'messages',
        title_key: 'limitPinnedPerAccount', hint_key: 'limitPinnedPerAccountDesc',
        unit_key: 'limitUnitMessages', default: 20, min: 1, max: 1000, value: 12,
      },
      {
        key: limits.KEYS.undoSendMax, section: 'sending',
        title_key: 'limitUndoSendMax', hint_key: 'limitUndoSendMaxDesc',
        unit_key: 'limitUnitSeconds', default: 60, min: 1, max: 3600, value: 45,
      },
    ],
  };
}

test('интерфейс берёт значение и границы предела из ядра, а своих чисел не держит', () => {
  limits.applyLimits(corePayload());
  assert.equal(limits.limitValue(limits.KEYS.pinnedPerAccount), 12,
    'рабочее значение взято не из ответа ядра');
  assert.equal(limits.limitSpec(limits.KEYS.undoSendMax).max, 3600);
  // Пока перечень не загружен, значения нет вовсе: число "на всякий случай" и
  // было той самой второй копией предела, расходившейся с ядром.
  limits.applyLimits({sections: [], limits: []});
  assert.equal(limits.limitValue(limits.KEYS.pinnedPerAccount), null);
  assert.equal(limits.limitsLoaded(), false);
  assert.equal(limits.withinLimit(limits.KEYS.pinnedPerAccount, 100000), true,
    'незагруженный перечень отклонил значение: решение должно остаться за ядром');
});

test('значение вне границ отклоняется с объяснением, а не молча', () => {
  limits.applyLimits(corePayload());
  assert.equal(limits.withinLimit(limits.KEYS.pinnedPerAccount, 1), true);
  assert.equal(limits.withinLimit(limits.KEYS.pinnedPerAccount, 1000), true);
  assert.equal(limits.withinLimit(limits.KEYS.pinnedPerAccount, 0), false);
  assert.equal(limits.withinLimit(limits.KEYS.pinnedPerAccount, 1001), false);
  // Дробное и нечисловое значение полем ввода тоже допускаются, а пределом -
  // нет: половина письма не закрепляется.
  assert.equal(limits.withinLimit(limits.KEYS.pinnedPerAccount, '2.5'), false);
  assert.equal(limits.withinLimit(limits.KEYS.pinnedPerAccount, ''), false);

  // Отказ называет подпись настройки и обе её границы. Подписи приходят из
  // общего каталога локализации, поэтому текст собирается его же переводом.
  const catalog = {limitPinnedPerAccount: 'Сколько писем можно закрепить', limitUnitMessages: 'писем'};
  const text = limits.limitErrorText(limits.KEYS.pinnedPerAccount, 'ru', key => catalog[key] || key);
  assert.equal(text, '"Сколько писем можно закрепить": допустимы значения от 1 до 1000 (писем)');
});

test('принятое значение приходит от ядра и сразу становится рабочим', async () => {
  limits.applyLimits(corePayload());
  const sent = [];
  const bridge = {
    setLimitSetting: async (key, value) => {
      sent.push([key, value]);
      return value;
    },
  };
  const saved = await limits.saveLimit(bridge, limits.KEYS.pinnedPerAccount, '30');
  assert.deepEqual(sent, [[limits.KEYS.pinnedPerAccount, 30]], 'в ядро ушла строка, а не число');
  assert.equal(saved, 30);
  assert.equal(limits.limitValue(limits.KEYS.pinnedPerAccount), 30,
    'реестр остался со старым значением: следующая проверка шла бы по прежнему пределу');

  // Отказ ядра реестр не меняет: иначе интерфейс жил бы с числом, которого
  // ядро не принимало.
  const failing = {setLimitSetting: async () => { throw new Error('предел не принят'); }};
  await assert.rejects(() => limits.saveLimit(failing, limits.KEYS.pinnedPerAccount, 40));
  assert.equal(limits.limitValue(limits.KEYS.pinnedPerAccount), 30);
});

test('поля раздела разложены по разделам ядра, а не одним списком', () => {
  limits.applyLimits(corePayload());
  const sections = limits.limitSectionRows();
  assert.deepEqual(sections.map(section => section.id), ['messages', 'sending']);
  assert.deepEqual(sections.map(section => section.fields.length), [1, 1]);
  assert.equal(sections[0].titleKey, 'limitSectionMessages',
    'подпись раздела взята строкой, а не ключом каталога: английский интерфейс остался бы русским');
  assert.equal(sections[0].fields[0].key, limits.KEYS.pinnedPerAccount);
});
