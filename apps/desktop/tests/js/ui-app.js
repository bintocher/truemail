// Запуск интерфейса целиком для проверок.
// Модули интерфейса подключаются в index.html обычными тегами script и делят
// одну область имён; здесь они выполняются тем же порядком и на той же
// разметке, а подделаны только браузерные примитивы и мост команд. Так
// проверка идёт настоящим путём: разметка - обработчик - мост - перерисовка.
// См. apps/desktop/tests/js/fake-dom.js.
'use strict';
const vm = require('node:vm');
const {createEnvironment, readUi, FakeFile} = require('./fake-dom.js');

// Порядок подключения берём из самой разметки, а не переписываем списком:
// порядок значим (модули делят область имён), и переписанная копия разошлась
// бы с index.html незаметно.
function moduleOrder(html) {
  const found = [];
  const pattern = /<script\s+src="([^"?]+)/g;
  let match;
  while ((match = pattern.exec(html)) !== null) found.push(match[1]);
  // theme-init и bridge работают с настоящим окном Tauri: моста команд там
  // ещё нет, и в проверке они не участвуют. window-chrome управляет рамкой
  // окна операционной системы.
  const skip = new Set(['theme-init.js', 'bridge.js', 'modules/window-chrome.js', 'modules/app-version.js']);
  return found.filter(name => name.endsWith('.js') && !skip.has(name));
}

// Заглушка моста команд: каждый вызов записан, ответ задаётся по имени.
// Неописанная команда отвечает null, а не падает: проверке важен свой путь, и
// первый же посторонний вызов иначе уводил бы её в сторону.
function createBridge(responses, calls) {
  return new Proxy({}, {
    get(_target, name) {
      if (typeof name !== 'string') return undefined;
      return (...args) => {
        calls.push({name, args});
        const answer = responses[name];
        if (typeof answer === 'function') {
          try {
            return Promise.resolve(answer(...args));
          } catch (error) {
            return Promise.reject(error);
          }
        }
        if (answer !== undefined) return Promise.resolve(answer);
        // Команда, которой проверка не занимается, отвечает пустым перечнем:
        // при старте интерфейс перерисовывает свои разделы, и ответ null на
        // запрос перечня роняет фоновую перерисовку, уводя проверку в сторону
        // от её собственного пути. Запросы одиночного значения отвечают null.
        return Promise.resolve(/^(get|read|save|set|delete|remove|open|start|preview|change)/.test(name) ? null : []);
      };
    },
    has() {
      return true;
    },
  });
}

/*
  Запустить интерфейс.
  accounts/folders/messages/tags/contacts - данные ядра;
  bridge - ответы команд по имени;
  locale - язык, применяется настоящим applyWizardLanguage.
*/
function startApp(options = {}) {
  const html = readUi('index.html');
  const env = createEnvironment({html, lang: options.locale || 'ru'});
  const sandbox = env.sandbox;
  const calls = [];
  const responses = {...(options.bridge || {})};
  sandbox.tm = createBridge(responses, calls);
  // Запись настроек в ядро при старте проверке не нужна: интерфейс сам не
  // сохраняет язык, пока хранилище не готово.
  sandbox.tmStorageReady = false;
  sandbox.tmComposerReady = true;
  const context = vm.createContext(sandbox);
  const order = moduleOrder(html);
  order.forEach(name => vm.runInContext(readUi(name), context, {filename: `ui/${name}`}));

  const run = code => vm.runInContext(code, context, {filename: 'test-step'});
  // Значения, объявленные модулями через let, свойствами окна не становятся -
  // это обычные переменные общей области. Читаем и пишем их выражением в том
  // же контексте, а не подменой свойства sandbox.
  const get = name => run(name);
  const set = (name, value) => {
    sandbox.__testValue = value;
    run(`${name}=__testValue`);
  };
  const apply = values => Object.entries(values).forEach(([name, value]) => set(name, value));

  apply({
    coreAccounts: options.accounts || [],
    coreTags: options.tags || [],
    coreContacts: options.contacts || [],
    messages: options.messages || [],
  });
  if (options.folders) {
    sandbox.__testValue = options.folders;
    run('setCoreFolders(__testValue)');
  }
  if (options.currentFolderId !== undefined) set('currentFolderId', options.currentFolderId);
  // Пределы приходят из ядра командой limit_settings; здесь их подаёт
  // проверка, и значения нарочно не такие, как у ядра по умолчанию -
  // сработавший предел тогда доказывает, что модуль прочитал реестр.
  const applyLimits = values => {
    sandbox.__testValue = {
      sections: [{id: 'test', title_key: 'section:test', hint_key: 'hint:test'}],
      limits: Object.entries(values).map(([key, value]) => ({
        key, section: 'test', title_key: `title:${key}`, hint_key: `hint:${key}`,
        unit_key: `unit:${key}`, default: value, min: 0, max: 1000000, value,
      })),
    };
    run('limitsModel.applyLimits(__testValue)');
  };
  if (options.limits) applyLimits(options.limits);

  return {
    sandbox,
    context,
    document: env.document,
    body: env.body,
    selection: env.selection,
    calls,
    responses,
    run,
    get,
    set,
    apply,
    applyLimits,
    // Доиграть отложенные задачи интерфейса, не дожидаясь их срока.
    advanceTimers: env.advanceTimers,
    // Дождаться уже начатых обещаний: путь интерфейса асинхронный, и без
    // ожидания разметка ещё не готова.
    settle: async (rounds = 20) => {
      for (let index = 0; index < rounds; index += 1) {
        await new Promise(resolve => setTimeout(resolve, 0));
      }
    },
    // Настоящая смена языка со всей перерисовкой (i18n-onboarding.js).
    async setLanguage(locale) {
      await sandbox.window.localizationReady;
      sandbox.window.applyWizardLanguage(locale, false);
    },
    ready() {
      return sandbox.window.localizationReady;
    },
  };
}

module.exports = {startApp, moduleOrder, FakeFile};
