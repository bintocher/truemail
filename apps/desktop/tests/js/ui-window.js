// Подставное окно интерфейса для проверок. Разметка берётся из настоящего
// index.html, модули окна выполняются целиком в одном общем контексте в том же
// порядке, в каком их подключает окно, а место моста к ядру занимает счётчик
// вызовов: итог проверяется по тому, что уходит в ядро, а не по внутренним
// переменным модулей.
// Так проверяется настоящий путь: разметка - обработчик - мост. Снятая
// привязка обработчика, переименованный класс или потерянный признак data
// роняют проверки, которые строятся на этом окне.

'use strict';

const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {FakeNode, FakeEvent, parseHtml} = require('./fake-dom.js');

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
  const htmlNode = tree.querySelector('html') || new FakeNode('html', documentStub);
  tree.children.slice().forEach(child => documentStub.appendChild(child));
  documentStub.documentElement = htmlNode;
  htmlNode.lang = 'ru';
  documentStub.body = documentStub.querySelector('body') || documentStub.appendChild(new FakeNode('body', documentStub));
  documentStub.head = documentStub.querySelector('head') || new FakeNode('head', documentStub);
  documentStub.activeElement = null;
  documentStub.hidden = false;
  documentStub.visibilityState = 'visible';
  documentStub.getElementById = id => documentStub.descendants().find(node => node.attributes.id === id) || null;
  documentStub.createElement = tag => new FakeNode(tag, documentStub);
  documentStub.createDocumentFragment = () => new FakeNode('#fragment', documentStub);
  documentStub.createTextNode = value => {
    const node = new FakeNode('span', documentStub);
    node.text = String(value);
    return node;
  };
  documentStub.createRange = () => ({setStart() {}, setEnd() {}, collapse() {}, selectNodeContents() {}});
  documentStub.execCommand = () => true;
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
    (documentStub.listeners.get(`window:${type}`) || []).forEach(handler => handler(new FakeEvent(type, event)));
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
    byId: id => documentStub.getElementById(id),
    query: selector => documentStub.querySelector(selector),
    queryAll: selector => documentStub.querySelectorAll(selector),
    // Команды, ушедшие в ядро: итог действия пользователя смотрится по ним.
    commands: () => calls.map(call => call.command),
    callsOf: command => calls.filter(call => call.command === command),
  };
}

module.exports = {createUiWindow, createDocument, createClock, createBridge, readUiFile, uiDir, ALL_UI_MODULES};
