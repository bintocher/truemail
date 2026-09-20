// Проверки выхода из повторного мастера первичной настройки.
// Решение о выходе принимает wizard-exit.js, но сам выход идёт настоящим
// путём: единый обработчик Escape в calendar-contacts.js, состояние мастера из
// i18n-onboarding.js и функция closeWizard. Раньше проверялась только чистая
// функция, и снятая привязка обработчика, потерянный вызов closeWizard или
// забытая проверка поля ввода оставляли все проверки зелёными.
// Спецификация: specs/setup-wizard-exit.md.
// Запуск: node --test apps/desktop/tests/js/wizard-exit.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {createUiWindow} = require('./ui-window.js');
const {wizardExitAllowed, wizardEscapeAction} = require('../../ui/modules/wizard-exit.js');

// Мастер открывается тем же вызовом, что и кнопка "Повторить настройку" в
// настройках: признак завершения спрашивается у ядра, поэтому подставной ответ
// getSetting решает, обязательный это мастер или повторный.
async function openWizard(onboardingCompleted) {
  const ui = createUiWindow({
    answers: {
      getSetting: key => (key === 'onboarding_completed' ? onboardingCompleted : null),
      allSettings: () => ({}),
    },
  });
  await ui.clock.drain();
  // Возвратное представление: мастер открывают из настроек, туда же и
  // возвращаются. Подстановка mailView скрыла бы потерю возвратного вида.
  ui.evaluate('showView("settingsView")');
  ui.evaluate('showWizard(5)');
  await ui.clock.drain();
  return ui;
}

const wizardOpen = ui => ui.byId('welcomeView').classes.has('active');
// Нажатие идёт с узла списка писем, а не с самого документа: так событие
// проходит всю цепочку всплытия, как в окне.
const pressEscape = async (ui, node) => {
  (node || ui.byId('msgs')).dispatch('keydown', {key: 'Escape'});
  await ui.clock.drain();
};

test('S-001, S-002, S-007: выход разрешает только сохранённое значение "true"', () => {
  // Значение приходит из ядра строкой. Приведение типов здесь открывало бы
  // выход из обязательного мастера: строка "1" и логическое true - не признак
  // завершённой настройки.
  const cases = [
    ['true', true, 'завершённая настройка не даёт выхода'],
    ['false', false, 'незавершённая настройка выпускает из мастера'],
    ['', false, 'пустое значение сочтено завершённой настройкой'],
    [null, false, 'отсутствующая запись сочтена завершённой настройкой'],
    [undefined, false, 'ошибка чтения настройки сочтена завершённой настройкой'],
    ['1', false, 'строка "1" сочтена завершённой настройкой'],
    [true, false, 'логическое true сочтено завершённой настройкой'],
  ];
  cases.forEach(([saved, expected, why]) => {
    assert.equal(wizardExitAllowed(saved), expected, `${why}: ${JSON.stringify(saved)}`);
  });
});

test('S-004, S-005, S-011, S-013: решение по Escape зависит от мастера, разрешения выхода и поля ввода', () => {
  // Меню и модальные окна до этой функции не доходят: единый обработчик
  // закрывает их раньше и выходит. Здесь остаются только те ветки, в которые
  // программа действительно заходит.
  const cases = [
    [{}, 'none', 'Escape без открытого мастера что-то закрывает'],
    [{wizardOpen: true, wizardExitAvailable: true}, 'wizard', 'повторный мастер не закрывается по Escape'],
    [{wizardOpen: true, wizardExitAvailable: false}, 'none', 'обязательный мастер закрывается по Escape'],
    [{wizardOpen: true, wizardExitAvailable: true, editingField: true}, 'none',
      'Escape в поле ввода мастера закрывает весь мастер и теряет набранное'],
    [{wizardOpen: false, wizardExitAvailable: true, editingField: true}, 'none',
      'закрытый мастер закрывается повторно'],
  ];
  cases.forEach(([state, expected, why]) => {
    assert.equal(wizardEscapeAction(state), expected, `${why}: ${JSON.stringify(state)}`);
  });
});

test('S-001, S-003, S-004: Escape в повторном мастере возвращает в то представление, откуда мастер открыли', async () => {
  const ui = await openWizard('true');
  assert.equal(wizardOpen(ui), true, 'мастер не открылся: проверять выход не на чем');
  const close = ui.byId('wzClose');
  assert.equal(close.classes.has('hidden'), false, 'кнопка выхода спрятана после завершённой настройки');
  assert.equal(close.disabled, false, 'кнопка выхода недоступна после завершённой настройки');

  await pressEscape(ui);
  assert.equal(wizardOpen(ui), false, 'Escape не закрыл повторный мастер');
  assert.equal(ui.byId('settingsView').classes.has('active'), true,
    'после выхода открылось не то представление, из которого мастер открывали');
  // Выход не завершает настройку: запись onboarding_completed трогать нечем.
  assert.deepEqual(ui.callsOf('setSetting'), [],
    'выход из мастера записал настройку в ядро: незавершённая настройка сочлась бы завершённой');
});

test('S-002, S-005: Escape в обязательном мастере оставляет его открытым', async () => {
  const ui = await openWizard(null);
  assert.equal(wizardOpen(ui), true, 'обязательный мастер не открылся');
  const close = ui.byId('wzClose');
  assert.equal(close.classes.has('hidden'), true, 'в обязательном мастере показана кнопка выхода');
  assert.equal(close.disabled, true, 'в обязательном мастере кнопка выхода доступна для нажатия');

  await pressEscape(ui);
  assert.equal(wizardOpen(ui), true, 'Escape выпустил из обязательного мастера: настройку можно пропустить');

  // Кнопка выхода ведёт в ту же функцию: обязательный мастер не выпускает и по
  // ней, даже если признак disabled в разметке потеряется.
  ui.evaluate('window.closeWizard()');
  await ui.clock.drain();
  assert.equal(wizardOpen(ui), true, 'обязательный мастер закрылся вызовом выхода');
});

test('S-013: Escape в поле ввода мастера отменяет ввод, а не весь мастер', async () => {
  const ui = await openWizard('true');
  const email = ui.byId('wzEmail');
  assert.ok(email, 'в мастере нет поля адреса: проверять нечего');
  email.focus();
  await pressEscape(ui, email);
  assert.equal(wizardOpen(ui), true,
    'Escape в поле ввода закрыл мастер: введённый адрес или код подтверждения теряется от случайного нажатия');

  // Тот же мастер, но курсор уже вне поля - нажатие доходит до мастера.
  email.blur();
  await pressEscape(ui);
  assert.equal(wizardOpen(ui), false, 'Escape вне поля ввода не закрыл повторный мастер');
});

test('S-006: открытое поверх мастера окно закрывается первым, мастер - только следующим нажатием', async () => {
  const ui = await openWizard('true');
  // Палитра команд открывается своим сочетанием и ложится поверх мастера.
  ui.byId('msgs').dispatch('keydown', {key: 'k', code: 'KeyK', ctrlKey: true});
  await ui.clock.drain();
  assert.equal(ui.queryAll('.overlay.open').length, 1, 'окно поверх мастера не открылось: проверять очередь нечем');

  await pressEscape(ui);
  assert.equal(ui.queryAll('.overlay.open').length, 0, 'первое нажатие не закрыло верхнее окно');
  assert.equal(wizardOpen(ui), true,
    'одно нажатие закрыло и окно, и мастер: пользователь теряет мастер вместе с окном');

  await pressEscape(ui);
  assert.equal(wizardOpen(ui), false, 'второе нажатие не дошло до мастера');
});
