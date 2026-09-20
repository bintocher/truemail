// Проверки полей получателей композера: закрытие поля "Копия" и "Скрытая
// копия" спрашивает про все адреса поля, согласие убирает их целиком, отказ
// оставляет поле открытым. Проверяется настоящий путь: разметка полей берётся
// из index.html, модуль композера выполняется целиком, а действия идут теми же
// обработчиками, которые вызовет браузер, - кнопкой поля, вводом адреса и
// уходом курсора из строки ввода. Итог смотрим по запросу на отправку: именно
// он уходит в ядро.
// Спецификация: specs/composer-recipient-fields.md.
// Запуск: node --test apps/desktop/tests/js/composer-recipient-fields.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const personSearch = require('../../ui/modules/person-search.js');
const recipientHistoryModel = require('../../ui/modules/recipient-history.js');

const uiDir = path.join(__dirname, '..', '..', 'ui');

// Разметка полей получателей берётся из настоящего файла окна: модуль ищет свои
// узлы по этим классам и признакам data, и подделанная разметка скрыла бы
// расхождение между кодом и окном.
function composerMarkup() {
  const html = fs.readFileSync(path.join(uiDir, 'index.html'), 'utf8');
  const start = html.indexOf('<div class="compbody">');
  const end = html.indexOf('<div class="compedit">', start);
  assert.ok(start > 0 && end > start, 'в index.html не нашлись поля композера');
  return html.slice(start, end);
}

const VOID_TAGS = new Set(['input', 'br', 'img', 'hr']);
const camel = name => name.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());

class FakeNode {
  constructor(tag) {
    this.tag = tag;
    this.children = [];
    this.classes = new Set();
    this.dataset = {};
    this.attributes = {};
    this.listeners = new Map();
    this.textContent = '';
    this.value = '';
    this.parentElement = null;
    this.focused = false;
    this.onclick = null;
  }

  get classList() {
    return {
      add: name => this.classes.add(name),
      remove: name => this.classes.delete(name),
      contains: name => this.classes.has(name),
      toggle: (name, on) => {
        const wanted = on === undefined ? !this.classes.has(name) : Boolean(on);
        if (wanted) this.classes.add(name); else this.classes.delete(name);
        return wanted;
      },
    };
  }

  set className(value) {
    this.classes = new Set(String(value || '').split(/\s+/).filter(Boolean));
  }

  get className() { return [...this.classes].join(' '); }

  set innerHTML(value) {
    // Разметку внутрь узла в проверяемых путях не пишут: её ставят пустой, чтобы
    // очистить. Непустое значение оставляем одним текстовым узлом.
    this.children = [];
    this.textContent = value ? String(value) : '';
  }

  get innerHTML() {
    return this.children.length ? this.children.map(child => child.innerHTML || child.textContent).join('') : this.textContent;
  }

  get innerText() { return this.innerHTML; }

  appendChild(node) {
    node.parentElement = this;
    this.children.push(node);
    return node;
  }

  setAttribute(name, value) { this.attributes[name] = String(value); }

  getAttribute(name) { return this.attributes[name] ?? null; }

  addEventListener(type, handler) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(handler);
  }

  dispatch(type, event = {}) {
    (this.listeners.get(type) || []).forEach(handler => handler(event));
  }

  focus() { this.focused = true; }

  descendants() {
    return this.children.flatMap(child => [child, ...child.descendants()]);
  }

  querySelector(selector) { return this.descendants().find(node => matches(node, selector)) || null; }

  querySelectorAll(selector) { return this.descendants().filter(node => matches(node, selector)); }

  closest(selector) {
    let node = this;
    while (node) {
      if (matches(node, selector)) return node;
      node = node.parentElement;
    }
    return null;
  }

  cloneNode() {
    const copy = new FakeNode(this.tag);
    copy.textContent = this.textContent;
    return copy;
  }
}

function matches(node, selector) {
  const attribute = selector.match(/^\[([\w-]+)(?:="([^"]*)")?\]$/);
  if (attribute) {
    const value = node.dataset[camel(attribute[1].replace(/^data-/, ''))];
    return value !== undefined && (attribute[2] === undefined || value === attribute[2]);
  }
  if (selector.startsWith('.')) return node.classes.has(selector.slice(1));
  return node.tag === selector;
}

function parseMarkup(html) {
  const root = new FakeNode('root');
  const stack = [root];
  const tagPattern = /<(\/?)([a-zA-Z][\w-]*)((?:\s+[^>]*?)?)(\/?)>/g;
  let match;
  while ((match = tagPattern.exec(html)) !== null) {
    const tag = match[2].toLowerCase();
    if (match[1] === '/') {
      if (stack.length > 1) stack.pop();
      continue;
    }
    const node = new FakeNode(tag);
    const attributePattern = /([\w-]+)(?:="([^"]*)")?/g;
    let attribute;
    while ((attribute = attributePattern.exec(match[3] || '')) !== null) {
      const name = attribute[1];
      const value = attribute[2] === undefined ? '' : attribute[2];
      if (name === 'class') node.className = value;
      else if (name.startsWith('data-')) node.dataset[camel(name.slice(5))] = value;
      else node.attributes[name] = value;
      if (name === 'value') node.value = value;
    }
    stack.at(-1).appendChild(node);
    if (!VOID_TAGS.has(tag) && match[4] !== '/') stack.push(node);
  }
  return root;
}

// Композер целиком: своих экспортов у него нет, поэтому проверяется он так же,
// как работает в окне - выполнением файла в подставном окружении.
function loadComposer() {
  const tree = parseMarkup(composerMarkup());
  const spare = new Map();
  const confirms = [];
  let confirmAnswer = true;
  const findById = id => tree.descendants().find(node => node.attributes.id === id) || null;
  const documentStub = {
    documentElement: {lang: 'ru'},
    getElementById: id => {
      const found = findById(id);
      if (found) return found;
      // Кнопки и поля за пределами блока получателей модулю тоже нужны: на них
      // он вешает свои обработчики при загрузке.
      if (!spare.has(id)) spare.set(id, new FakeNode('div'));
      return spare.get(id);
    },
    querySelector: selector => tree.querySelector(selector),
    querySelectorAll: selector => tree.querySelectorAll(selector),
    addEventListener: () => {},
    createElement: tag => new FakeNode(tag),
    createRange: () => ({setStart() {}, collapse() {}}),
    body: new FakeNode('body'),
  };
  const context = {
    console: {log() {}, info() {}, warn() {}, error() {}},
    document: documentStub,
    setTimeout: () => 0,
    clearTimeout: () => {},
    setInterval: () => 0,
    clearInterval: () => {},
    Event: class { constructor(type) { this.type = type; } },
    MouseEvent: class { constructor(type) { this.type = type; } },
    personSearch,
    coreContacts: [],
    coreAccounts: [{id: 1, email: 'me@example.test'}],
    wizardLocale: 'ru',
    activeMessage: null,
    activeFullMessage: null,
    messages: [],
    selectedMessageIds: new Set(),
    L: (russian) => russian,
    showToast: () => {},
    showView: () => {},
    escapeHtml: value => String(value ?? ''),
    confirmAction: async question => { confirms.push(question); return confirmAnswer; },
    // Соседи композера по общей области имён: он вешает их на кнопки списка
    // писем при загрузке, а к полям получателей они отношения не имеют.
    selectAllCurrentMessages: () => {},
    clearMessageSelection: () => {},
    performMessageAction: () => {},
  };
  context.window = context;
  context.globalThis = context;
  context.window.addEventListener = () => {};
  context.window.recipientHistoryModel = recipientHistoryModel;
  context.window.composerBody = require('../../ui/modules/composer-body.js');
  context.window.tm = {setSetting: async () => {}, listSignatures: async () => []};
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(uiDir, 'modules', 'composer.js'), 'utf8'), context);
  const evaluate = expression => vm.runInContext(expression, context);
  return {
    tree,
    confirms,
    answerConfirm: value => { confirmAnswer = value; },
    input: id => findById(id),
    field: id => tree.querySelector(`[data-recipient-field="${id}"]`),
    chips: id => findById(id).parentElement.querySelector('.recipient-chips').children.length,
    hideButton: id => tree.querySelector(`[data-recipient-hide="${id}"]`),
    showButton: id => tree.querySelector(`[data-recipient-toggle="${id}"]`),
    evaluate,
    // Запрос на отправку рождается внутри подставного окружения, поэтому
    // перечни адресов переносим в обычные массивы: иначе сравнение спорит о
    // происхождении массива, а не о его содержимом.
    request: () => {
      const built = evaluate('composerRequest()');
      return {to: Array.from(built.to), cc: Array.from(built.cc), bcc: Array.from(built.bcc)};
    },
  };
}

// Адрес вводится и завершается так же, как в окне: текст в строке ввода и уход
// курсора из неё. Завершённый адрес становится плашкой, строка ввода пустеет.
function typeAddress(composer, id, address) {
  const input = composer.input(id);
  input.value = address;
  input.dispatch('blur');
}

test('S-001, S-002, S-004: закрытие поля "Копия" спрашивает про все его адреса и убирает их', async () => {
  const composer = loadComposer();
  composer.showButton('compCc').onclick();
  typeAddress(composer, 'compTo', 'boss@example.test');
  typeAddress(composer, 'compCc', 'rival@example.test');
  assert.equal(composer.chips('compCc'), 1, 'адрес не стал плашкой: проверять нечего');
  assert.equal(composer.input('compCc').value, '', 'строка ввода не очистилась после завершения адреса');

  composer.answerConfirm(true);
  await composer.hideButton('compCc').onclick();
  assert.equal(composer.confirms.length, 1,
    'крестик закрыл поле с адресом без вопроса: убранный с глаз получатель уходит в письмо незаметно');
  assert.deepEqual(composer.request().cc, [],
    'адрес остался в письме после закрытия поля: получатель, убранный с глаз, получит письмо');
  assert.equal(composer.chips('compCc'), 0, 'плашка осталась в разметке: поле откроется с прежним адресом');
  assert.ok(composer.field('compCc').classes.has('hidden'), 'поле осталось открытым после согласия');

  // S-004: пустое поле закрывается молча - спрашивать там не о чем.
  composer.showButton('compCc').onclick();
  await composer.hideButton('compCc').onclick();
  assert.equal(composer.confirms.length, 1, 'пустое поле спросило про очистку адресов');
  assert.ok(composer.field('compCc').classes.has('hidden'), 'пустое поле не закрылось');
});

test('S-003: отказ оставляет поле открытым, а все его адреса на месте', async () => {
  const composer = loadComposer();
  composer.showButton('compBcc').onclick();
  typeAddress(composer, 'compTo', 'boss@example.test');
  typeAddress(composer, 'compBcc', 'secret@example.test');
  // Второй адрес остаётся недовведённым в строке: отказ обязан сохранить и его.
  composer.input('compBcc').value = 'draft@example.test';

  composer.answerConfirm(false);
  await composer.hideButton('compBcc').onclick();
  assert.equal(composer.confirms.length, 1, 'вопроса не было: отказаться от очистки поля негде');
  assert.equal(composer.field('compBcc').classes.has('hidden'), false,
    'поле закрылось вопреки отказу');
  assert.deepEqual(composer.request().bcc, ['secret@example.test', 'draft@example.test'],
    'после отказа адреса поля пропали');

  // Согласие убирает и завершённый адрес, и недовведённый остаток строки ввода.
  composer.answerConfirm(true);
  await composer.hideButton('compBcc').onclick();
  assert.deepEqual(composer.request().bcc, [], 'после согласия адреса поля остались в письме');
  assert.equal(composer.input('compBcc').value, '', 'строка ввода не очистилась');
  assert.equal(composer.chips('compBcc'), 0, 'плашки не перерисовались');
});

test('S-005: заданный набор адресов поля не смешивается с прежним вводом', async () => {
  const composer = loadComposer();
  composer.showButton('compCc').onclick();
  typeAddress(composer, 'compTo', 'boss@example.test');
  // Недовведённый адрес в строке: черновик восстанавливается поверх открытого
  // композера, и остаток прежней строки ввода ушёл бы в письмо вместе с
  // восстановленными адресами.
  composer.input('compCc').value = 'leftover@example.test';
  composer.evaluate('setRecipients("compCc", "copy@example.test")');
  assert.deepEqual(composer.request().cc, ['copy@example.test'],
    'к заданным адресам поля примешался остаток прежней строки ввода');

  // Сброс композера очищает поле тем же способом.
  composer.input('compCc').value = 'leftover@example.test';
  composer.evaluate('resetComposer()');
  typeAddress(composer, 'compTo', 'boss@example.test');
  assert.deepEqual(composer.request().cc, [], 'сброс композера оставил адрес в поле "Копия"');
});
