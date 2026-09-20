// Проверки решений, которые принимает карточка ящика в настройках: какая
// карточка раскрыта, что восстанавливается из хранилища браузера, какие
// действия положены способу входа и как называются числа папок и календарей.
// Настоящая разметка и клики проверяются отдельно - в account-cards-dom.test.js.
// Спецификация: specs/accounts-accordion-password.md.
// Запуск: node --test apps/desktop/tests/js/account-cards.test.js (Node 22+).

'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {
  nextOpenAccountId,
  restoreOpenAccountId,
  canChangeAccountPassword,
  isOAuthAccount,
  accountStatsText,
} = require('../../ui/modules/account-cards.js');

test('S-002: раскрытой остаётся одна карточка, повторное нажатие сворачивает её', () => {
  // Полный цикл переключений: раскрыть, перейти к соседней, вернуться и
  // свернуть. Проверка по одному шагу пропускала бы аккордеон, который
  // раскрывает вторую карточку, не закрыв первую.
  let open = null;
  const press = id => { open = nextOpenAccountId(open, id); return open; };
  assert.equal(press(1), 1, 'первое нажатие не раскрыло карточку');
  assert.equal(press(2), 2, 'переход к соседней карточке не переключил раскрытую');
  assert.equal(press(2), null, 'повторное нажатие по раскрытой карточке не свернуло её');
  assert.equal(press(2), 2, 'свёрнутая карточка не раскрывается заново');
  assert.equal(press(1), 1, 'возврат к первой карточке не переключил раскрытую');
});

test('S-001, S-003, S-005: из хранилища восстанавливается только существующий ящик', () => {
  const ids = [1, 2, 3];
  const cases = [
    ['2', ids, 2, 'сохранённая карточка не восстановилась'],
    [2, ids, 2, 'сохранённое число не восстановилось'],
    [null, ids, null, 'пустое хранилище раскрыло карточку'],
    [undefined, ids, null, 'нечитаемое хранилище раскрыло карточку'],
    ['', ids, null, 'пустая запись раскрыла карточку'],
    ['99', ids, null, 'восстановлен ящик, которого больше нет'],
    ['not-a-number', ids, null, 'испорченная запись хранилища принята за номер ящика'],
    ['{"broken":true}', ids, null, 'обрывок JSON принят за номер ящика'],
    ['1', [], null, 'карточка восстановлена при пустом списке ящиков'],
    ['1', null, null, 'карточка восстановлена без списка ящиков'],
  ];
  cases.forEach(([saved, accounts, expected, why]) => {
    assert.equal(restoreOpenAccountId(saved, accounts), expected,
      `${why}: запись ${JSON.stringify(saved)}, ящики ${JSON.stringify(accounts)}`);
  });
});

test('S-008: способ входа решает, что показывать в карточке - смену пароля или повторный вход', () => {
  // Обе кнопки выводятся из одного поля auth_kind, и ошибка в любой из них
  // одинаково опасна: OAuth-ящику предлагать пароль бессмысленно, а парольному
  // ящику повторный вход через браузер - тем более.
  const cases = [
    ['password', true, false],
    ['app_password', true, false],
    ['ntlm', true, false],
    ['oauth2', false, true],
    ['unknown_kind', false, false],
    ['', false, false],
    [null, false, false],
    [undefined, false, false],
  ];
  cases.forEach(([kind, password, oauth]) => {
    assert.equal(canChangeAccountPassword(kind), password,
      `смена пароля предложена неверно для способа входа ${JSON.stringify(kind)}`);
    assert.equal(isOAuthAccount(kind), oauth,
      `повторный вход предложен неверно для способа входа ${JSON.stringify(kind)}`);
  });
});

test('issue 79: число папок и календарей согласовано на обоих языках', () => {
  const cases = [
    [1, 1, 'ru', '1 папка · 1 календарь'],
    [2, 2, 'ru', '2 папки · 2 календаря'],
    [5, 5, 'ru', '5 папок · 5 календарей'],
    // 11-14 - форма "много", несмотря на окончание на 1-4.
    [11, 11, 'ru', '11 папок · 11 календарей'],
    [21, 14, 'ru', '21 папка · 14 календарей'],
    // 101 оканчивается на 1 и в особый десяток не попадает - снова "один".
    [101, 101, 'ru', '101 папка · 101 календарь'],
    [0, 0, 'ru', '0 папок · 0 календарей'],
    [1, 2, 'en', '1 folder · 2 calendars'],
    [3, 1, 'en', '3 folders · 1 calendar'],
    [0, 0, 'en', '0 folders · 0 calendars'],
  ];
  cases.forEach(([folders, calendars, language, expected]) => {
    assert.equal(accountStatsText(folders, calendars, language), expected,
      `подпись карточки собрана неверно: ${folders} и ${calendars} на языке ${language}`);
  });
});
