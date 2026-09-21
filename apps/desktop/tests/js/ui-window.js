// Подставное окно интерфейса для проверок. Разметка берётся из настоящего
// index.html, модули окна выполняются целиком в одном общем контексте в том же
// порядке, в каком их подключает окно, а место моста к ядру занимает счётчик
// вызовов: итог проверяется по тому, что уходит в ядро, а не по внутренним
// переменным модулей.
// Так проверяется настоящий путь: разметка - обработчик - мост. Снятая
// привязка обработчика, переименованный класс или потерянный признак data
// роняют проверки, которые строятся на этом окне.

// Обвязка в проверках интерфейса одна: и createUiWindow, и startApp поднимают
// окно этим же путём. Поведение узлов, событий и выделения живёт в общем
// мини-DOM (fake-dom.js) и правится там, а не здесь.
'use strict';

const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {
  FakeNode, FakeText, FakeRange, FakeEvent, parseHtml, parseMarkup, createSelection,
  createFileReaderClass,
} = require('./fake-dom.js');

const uiDir = path.join(__dirname, '..', '..', 'ui');

function readUiFile(relative) {
  return fs.readFileSync(path.join(uiDir, relative), 'utf8');
}

// Порядок подключения модулей читается из самого index.html: список, записанный
// в проверке отдельно, разошёлся бы с окном молча, а модули видят друг друга
// именно в этом порядке (mail.js зовёт L из smart-rules.js, а список писем -
// sortMenu из settings.js).
function uiModuleOrder() {
  const html = readUiFile('index.html');
  const order = [];
  const pattern = /<script src="(modules\/[\w-]+\.js)[^"]*"><\/script>/g;
  let match;
  while ((match = pattern.exec(html)) !== null) order.push(match[1]);
  if (!order.length) throw new Error('в index.html не нашлись подключения модулей интерфейса');
  return order;
}

const ALL_UI_MODULES = uiModuleOrder();

// Виртуальные часы: сроки интерфейса измеряются секундами и минутами, ждать их
// по-настоящему значит не проверять их вовсе.
function createClock(startTime = 0) {
  let now = startTime;
  let sequence = 0;
  const jobs = new Map();
  const drain = async () => { for (let step = 0; step < 50; step += 1) await Promise.resolve(); };
  return {
    now: () => now,
    setTimeout: (fn, ms) => {
      const id = (sequence += 1);
      jobs.set(id, {at: now + Math.max(0, Number(ms) || 0), fn, every: 0});
      return id;
    },
    setInterval: (fn, ms) => {
      const id = (sequence += 1);
      const every = Math.max(1, Number(ms) || 0);
      jobs.set(id, {at: now + every, fn, every});
      return id;
    },
    clearTimeout: id => jobs.delete(id),
    clearInterval: id => jobs.delete(id),
    async advance(ms) {
      const target = now + ms;
      for (;;) {
        let next = null;
        for (const [id, job] of jobs) {
          if (job.at <= target && (next === null || job.at < next.job.at)) next = {id, job};
        }
        if (!next) break;
        now = next.job.at;
        if (next.job.every) next.job.at = now + next.job.every;
        else jobs.delete(next.id);
        try { next.job.fn(); } catch (error) { void error; }
        await drain();
      }
      now = target;
      await drain();
    },
    drain,
  };
}

// Документ окна: дерево из index.html плюс поведение, на которое опираются
// модули (поиск узлов, создание элементов, всплытие событий до документа).
function createDocument(html) {
  const documentStub = new FakeNode('#document');
  documentStub.nodeType = 9;
  documentStub.ownerDocument = documentStub;
  const tree = parseHtml(html, documentStub);
  const htmlNode = tree.querySelector('html') || new FakeNode('html');
  tree.children.slice().forEach(child => documentStub.appendChild(child));
  documentStub.documentElement = htmlNode;
  htmlNode.lang = 'ru';
  documentStub.body = documentStub.querySelector('body') || documentStub.appendChild(new FakeNode('body'));
  documentStub.head = documentStub.querySelector('head') || new FakeNode('head');
  documentStub.activeElement = null;
  documentStub.hidden = false;
  documentStub.visibilityState = 'visible';
  documentStub.getElementById = id => documentStub.descendants().find(node => node.attributes.id === id) || null;
  documentStub.createElement = tag => new FakeNode(tag);
  documentStub.createDocumentFragment = () => {
    const fragment = new FakeNode('#fragment');
    fragment.nodeType = 11;
    fragment.isFragment = true;
    return fragment;
  };
  documentStub.createTextNode = value => new FakeText(value);
  documentStub.createRange = () => new FakeRange();
  // Вставка разметки тем же путём, каким её делает редактор письма: картинка
  // встаёт в текущий диапазон выделения, а не дописывается в конец тела.
  documentStub.execCommand = (command, _ui, value) => {
    if (command !== 'insertHTML') return true;
    const range = documentStub.selection && documentStub.selection.current();
    if (!range) return false;
    range.insertNodes(parseMarkup(String(value ?? '')));
    return true;
  };
  documentStub.elementFromPoint = () => null;
  return documentStub;
}

// Ответы, без которых окно не доживает до проверяемого действия: эти команды
// зовутся при загрузке и при перерисовке разделов, а перечень они ждут всегда.
// Ответ, заданный проверкой, всегда сильнее этого набора.
const DEFAULT_ANSWERS = {
  listLabels: () => [],
  listPinnedMessages: () => ({messages: [], ordinary: []}),
  messageLabelIds: () => [],
  pendingMailRuleRuns: () => [],
  failedMessageOperations: () => [],
  listMailRules: () => [],
  pendingSenderPolicyJobs: () => [],
  pendingSenderSweepJobs: () => [],
  listOutbox: () => [],
  listRecipientHistory: () => [],
  overdueMessageTaskCount: () => 0,
  allSettings: () => ({}),
};

// Мост к ядру: команды отвечают заданными значениями, а вызовы запоминаются -
// именно по ним проверяется итог действия пользователя.
function createBridge(answers = {}) {
  const calls = [];
  const handlers = {...DEFAULT_ANSWERS, ...answers};
  const bridge = new Proxy({}, {
    get(_target, name) {
      if (name === 'calls') return calls;
      if (name === 'callsOf') return command => calls.filter(call => call.command === command);
      if (name === 'lastCall') return command => calls.filter(call => call.command === command).at(-1) || null;
      if (name === 'answer') return (command, handler) => { handlers[command] = handler; };
      if (typeof name !== 'string') return undefined;
      return async (...args) => {
        calls.push({command: name, args});
        const handler = handlers[name];
        if (typeof handler === 'function') return handler(...args);
        return handler === undefined ? null : handler;
      };
    },
    has() { return true; },
  });
  return {bridge, calls};
}

function createStorage() {
  const map = new Map();
  return {
    map,
    getItem: key => (map.has(String(key)) ? map.get(String(key)) : null),
    setItem: (key, value) => map.set(String(key), String(value)),
    removeItem: key => map.delete(String(key)),
    clear: () => map.clear(),
  };
}

// Окно целиком: контекст с заглушками, дерево страницы и перечисленные модули,
// выполненные в порядке подключения.
function createUiWindow(options = {}) {
  const {
    modules = ALL_UI_MODULES,
    answers = {},
    locale = 'ru',
    clock = createClock(Date.parse('2026-09-20T10:00:00Z')),
    globals = {},
    html = readUiFile('index.html'),
  } = options;

  const documentStub = createDocument(html);
  documentStub.documentElement.lang = locale;
  const {bridge, calls} = createBridge(answers);
  const toasts = [];
  const storage = createStorage();
  const selection = createSelection();
  documentStub.selection = selection;

  const context = {
    console: {log() {}, info() {}, warn() {}, error() {}, debug() {}},
    document: documentStub,
    performance: {now: () => clock.now()},
    setTimeout: clock.setTimeout,
    clearTimeout: clock.clearTimeout,
    setInterval: clock.setInterval,
    clearInterval: clock.clearInterval,
    queueMicrotask: fn => Promise.resolve().then(fn),
    requestAnimationFrame: fn => clock.setTimeout(() => fn(clock.now()), 16),
    cancelAnimationFrame: id => clock.clearTimeout(id),
    getComputedStyle: () => ({getPropertyValue: () => ''}),
    // Выделение в окне: композер вставляет разметку в текущий диапазон, и без
    // него путь вставки в проверке не воспроизводится.
    getSelection: () => selection,
    localStorage: storage,
    sessionStorage: createStorage(),
    navigator: {language: locale, clipboard: {writeText: async () => {}}, userAgent: 'node'},
    location: {href: 'tauri://localhost/index.html', reload: () => {}},
    matchMedia: () => ({matches: false, addEventListener() {}, removeEventListener() {}}),
    Event: FakeEvent,
    MouseEvent: FakeEvent,
    KeyboardEvent: FakeEvent,
    CustomEvent: FakeEvent,
    Image: class { constructor() { this.src = ''; } },
    // Наблюдатель за разметкой нужен доступности: в проверках его работа не
    // нужна, но без объявления модуль не загрузится вовсе.
    MutationObserver: class { observe() {} disconnect() {} takeRecords() { return []; } },
    ResizeObserver: class { observe() {} disconnect() {} },
    IntersectionObserver: class { observe() {} disconnect() {} },
    Blob: class { constructor(parts) { this.parts = parts; } },
    // Чтение вложенного файла: композер читает вставленную картинку им же.
    FileReader: createFileReaderClass(),
    // Кодирование base64: им интерфейс раскодирует имена папок IMAP
    // (модифицированный UTF-7) и читает вложения.
    atob: value => Buffer.from(String(value), 'base64').toString('binary'),
    btoa: value => Buffer.from(String(value), 'binary').toString('base64'),
    Buffer,
    URL: {createObjectURL: () => 'blob:stub', revokeObjectURL: () => {}},
    innerWidth: 1280,
    innerHeight: 800,
    devicePixelRatio: 1,
    alert: () => {},
    confirm: () => true,
    prompt: () => null,
    // Загрузка файла окном: переводы берутся с диска настоящими, поэтому
    // пропавший ключ перевода виден проверке смены языка.
    fetch: async url => {
      const relative = String(url).split('?')[0];
      const file = path.join(uiDir, relative);
      if (!fs.existsSync(file)) return {ok: false, status: 404, async json() { return {}; }, async text() { return ''; }};
      const body = fs.readFileSync(file, 'utf8');
      return {ok: true, status: 200, async json() { return JSON.parse(body); }, async text() { return body; }};
    },
  };
  context.window = context;
  context.globalThis = context;
  context.self = context;
  context.window.addEventListener = (type, handler) => documentStub.addEventListener(`window:${type}`, handler);
  context.window.removeEventListener = (type, handler) => documentStub.removeEventListener(`window:${type}`, handler);
  context.window.dispatchWindowEvent = (type, event = {}) => {
    (documentStub.listeners[`window:${type}`] || []).forEach(handler => handler(new FakeEvent(type, event)));
  };
  context.window.tm = bridge;
  context.window.__TAURI__ = {
    core: {invoke: async (command, args) => bridge[command](args)},
    event: {listen: async () => () => {}, emit: async () => {}},
    dialog: {open: async () => null, save: async () => null, message: async () => {}, ask: async () => true, confirm: async () => true},
    opener: {openUrl: async () => {}},
    window: {getCurrentWindow: () => ({listen: async () => () => {}, isMaximized: async () => false})},
  };
  context.window.showToast = (message, actionLabel, action) => { toasts.push({message, actionLabel, action}); };
  context.window.corePageSize = 50;
  Object.assign(context, globals);
  Object.assign(context.window, globals);

  vm.createContext(context);
  const loaded = [];
  modules.forEach(name => {
    const source = readUiFile(name.includes('/') ? name : path.join('modules', name));
    vm.runInContext(source, context, {filename: name});
    loaded.push(name);
  });

  const evaluate = expression => vm.runInContext(expression, context);
  return {
    context,
    document: documentStub,
    clock,
    bridge,
    calls,
    toasts,
    storage,
    loaded,
    evaluate,
    selection,
    byId: id => documentStub.getElementById(id),
    query: selector => documentStub.querySelector(selector),
    queryAll: selector => documentStub.querySelectorAll(selector),
    // Команды, ушедшие в ядро: итог действия пользователя смотрится по ним.
    commands: () => calls.map(call => call.command),
    callsOf: command => calls.filter(call => call.command === command),
  };
}


/*
  Запустить интерфейс с данными ядра - второй вход в ту же обвязку.

  Отличается от createUiWindow только подачей входных данных: значения ядра
  кладутся в общую область имён модулей, пределы подаются реестром, а язык
  переключается настоящим applyWizardLanguage. Само окно поднимается тем же
  путём, поэтому правка поведения окна одна на оба входа.

  accounts/folders/messages/tags/contacts - данные ядра;
  bridge - ответы команд по имени;
  locale - язык окна.
*/
function startApp(options = {}) {
  const ui = createUiWindow({
    answers: options.bridge || {},
    locale: options.locale || 'ru',
  });
  const {context, document: documentStub} = ui;
  // Запись настроек в ядро при старте проверке не нужна: интерфейс сам не
  // сохраняет язык, пока хранилище не готово.
  context.tmStorageReady = false;
  context.tmComposerReady = true;

  const run = ui.evaluate;
  // Значения, объявленные модулями через let, свойствами окна не становятся -
  // это обычные переменные общей области. Читаем и пишем их выражением в том
  // же контексте, а не подменой свойства окна.
  const get = name => run(name);
  const set = (name, value) => {
    context.__testValue = value;
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
    context.__testValue = options.folders;
    run('setCoreFolders(__testValue)');
  }
  if (options.currentFolderId !== undefined) set('currentFolderId', options.currentFolderId);
  // Пределы приходят из ядра командой limit_settings; здесь их подаёт
  // проверка, и значения нарочно не такие, как у ядра по умолчанию -
  // сработавший предел тогда доказывает, что модуль прочитал реестр.
  const applyLimits = values => {
    context.__testValue = {
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
    ...ui,
    sandbox: context,
    body: documentStub.body,
    run,
    get,
    set,
    apply,
    applyLimits,
    // Доиграть отложенные задачи интерфейса, не дожидаясь их срока.
    advanceTimers: ms => ui.clock.advance(ms),
    // Дождаться уже начатых обещаний: путь интерфейса асинхронный, и без
    // ожидания разметка ещё не готова.
    settle: async (rounds = 20) => {
      for (let index = 0; index < rounds; index += 1) {
        await new Promise(resolve => setTimeout(resolve, 0));
      }
    },
    // Настоящая смена языка со всей перерисовкой (i18n-onboarding.js).
    async setLanguage(locale) {
      await context.localizationReady;
      context.applyWizardLanguage(locale, false);
    },
    ready() {
      return context.localizationReady;
    },
  };
}

module.exports = {createUiWindow, startApp, createDocument, createClock, createBridge, readUiFile, uiDir, ALL_UI_MODULES};
