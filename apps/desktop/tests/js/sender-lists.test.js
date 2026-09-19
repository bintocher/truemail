// Проверки списков отправителей, игнорируемых переписок и автоочистки по
// отправителю: разбор значений и настоящий путь интерфейса - пункт меню,
// диалог, обращение к мосту команд и перерисовка разделов при смене языка.
// Браузерные примитивы здесь подделаны, а сами модули интерфейса выполняются
// настоящие: проверки, читающие только чистые функции, не замечают, что
// обработчик меню или перерисовка сломались.
// Спецификации: specs/blocked-senders.md, specs/ignore-conversation.md,
// specs/sweep-by-sender.md.
// Запуск: node --test apps/desktop/tests/js/sender-lists.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const senders = require('../../ui/modules/sender-lists.js');

const uiDir = path.join(__dirname, '..', '..', 'ui', 'modules');
const readModule = name => fs.readFileSync(path.join(uiDir, name), 'utf8');

// Разбор разметки диалога: нужны теги, классы и признаки data, потому что
// модуль ищет свои поля именно по ним. Вложенность узлов на поиск не влияет,
// поэтому дерево держим плоским.
const VOID_TAGS = new Set(['input', 'br', 'img', 'hr']);

function parseMarkup(html) {
  const root = [];
  const stack = [];
  const tagPattern = /<(\/?)([a-zA-Z][\w-]*)((?:\s+[^>]*?)?)(\/?)>/g;
  let match;
  while ((match = tagPattern.exec(html)) !== null) {
    const tag = match[2].toLowerCase();
    if (match[1] === '/') {
      const closed = stack.pop();
      // Значение списка по умолчанию - значение первого пункта, как в браузере.
      if (closed && closed.tag === 'select' && !closed.value) {
        const first = closed.children.find(child => child.tag === 'option');
        if (first) closed.value = first.value;
      }
      continue;
    }
    const node = new FakeNode(tag);
    const attributes = match[3] || '';
    const attributePattern = /([\w-]+)(?:="([^"]*)")?/g;
    let attribute;
    while ((attribute = attributePattern.exec(attributes)) !== null) {
      const name = attribute[1];
      const value = attribute[2] === undefined ? '' : attribute[2];
      if (name === 'class') node.className = value;
      else if (name.startsWith('data-')) node.dataset[name.slice(5)] = value;
      else node.attributes[name] = value;
      if (name === 'value') node.value = value;
      if (name === 'checked') node.checked = true;
    }
    const parent = stack.at(-1);
    if (parent) parent.appendChild(node);
    else root.push(node);
    if (!VOID_TAGS.has(tag) && match[4] !== '/') stack.push(node);
  }
  return root;
}

function matchesSelector(node, selector) {
  return selector.split(',').map(part => part.trim()).filter(Boolean).some(part => {
    if (part.startsWith('[') && part.endsWith(']')) {
      const name = part.slice(1, -1);
      return name.startsWith('data-') && node.dataset[camel(name.slice(5))] !== undefined;
    }
    return part.split('.').filter(Boolean).every((piece, index) =>
      index === 0 && !part.startsWith('.') ? node.tag === piece : node.classes.has(piece));
  });
}

const camel = name => name.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());

class FakeNode {
  constructor(tag) {
    this.tag = tag;
    this.children = [];
    this.classes = new Set();
    this.dataset = {};
    this.attributes = {};
    this.listeners = {};
    this.textContent = '';
    this.value = '';
    this.checked = false;
    this.parent = null;
  }

  get className() {
    return [...this.classes].join(' ');
  }

  set className(value) {
    this.classes = new Set(String(value).split(/\s+/).filter(Boolean));
  }

  get classList() {
    const classes = this.classes;
    return {
      add: name => classes.add(name),
      remove: name => classes.delete(name),
      contains: name => classes.has(name),
      toggle: (name, force) => (force ? classes.add(name) : classes.delete(name)),
    };
  }

  set innerHTML(html) {
    this.children = parseMarkup(html);
    this.children.forEach(child => {
      child.parent = this;
    });
  }

  get innerHTML() {
    return '';
  }

  appendChild(node) {
    node.parent = this;
    this.children.push(node);
    return node;
  }

  append(...nodes) {
    nodes.forEach(node => this.appendChild(node));
  }

  remove() {
    if (!this.parent) return;
    this.parent.children = this.parent.children.filter(child => child !== this);
    this.parent = null;
  }

  setAttribute(name, value) {
    this.attributes[name] = value;
  }

  addEventListener(type, handler) {
    (this.listeners[type] = this.listeners[type] || []).push(handler);
  }

  dispatch(type, event) {
    (this.listeners[type] || []).forEach(handler => handler(event));
  }

  closest(selector) {
    let node = this;
    while (node) {
      if (matchesSelector(node, selector)) return node;
      node = node.parent;
    }
    return null;
  }

  descendants() {
    return this.children.flatMap(child => [child, ...child.descendants()]);
  }

  querySelector(selector) {
    return this.querySelectorAll(selector)[0] || null;
  }

  querySelectorAll(selector) {
    return this.descendants().filter(node => matchesSelector(node, selector));
  }

  text() {
    return [this.textContent, ...this.descendants().map(node => node.textContent)]
      .filter(Boolean)
      .join(' ');
  }
}

// Общий запуск обоих модулей интерфейса в одном окружении: они делят область
// имён так же, как в index.html.
function startUi({policies = [], conversations = [], sweepRules = [], policyJobs = [], sweepJobs = []} = {}) {
  const hosts = new Map();
  const hostNames = [
    'senderPoliciesList', 'ignoredConversationsList', 'senderSweepList', 'senderPendingJobs',
    'threadMoreMenu', 'ctxmenu', 'senderPolicyAddBlocked', 'senderPolicyAddTrusted',
  ];
  hostNames.forEach(name => hosts.set(name, new FakeNode('div')));
  const body = new FakeNode('body');
  const calls = [];
  const record = (name, result) => (...args) => {
    calls.push({name, args});
    return Promise.resolve(typeof result === 'function' ? result(...args) : result);
  };
  const sandbox = {
    console,
    setTimeout,
    English: false,
    document: {
      body,
      createElement: tag => new FakeNode(tag),
      getElementById: id => hosts.get(id) || null,
    },
    escapeHtml: value => String(value ?? ''),
    renderIcons: () => {},
    showToast: () => {},
    confirmAction: () => Promise.resolve(true),
    prompt: () => '',
    showView: () => {},
    setSection: () => {},
    activeMessage: {id: 7, account_id: 3, from: {email: 'shop@example.test'}},
    coreAccounts: [{id: 3, email: 'me@example.test'}],
  };
  sandbox.window = sandbox;
  sandbox.smartIsEnglish = () => sandbox.English;
  sandbox.L = (ru, en) => (sandbox.English ? en : ru);
  sandbox.tm = {
    listSenderPolicies: record('listSenderPolicies', policies),
    listIgnoredConversations: record('listIgnoredConversations', conversations),
    listSenderSweepRules: record('listSenderSweepRules', sweepRules),
    pendingSenderPolicyJobs: record('pendingSenderPolicyJobs', policyJobs),
    pendingSenderSweepJobs: record('pendingSenderSweepJobs', sweepJobs),
    previewSenderPolicy: record('previewSenderPolicy', (kind, value) => ({
      kind, value, own_address: false, own_domain: false,
      snapshot_key: 'policy-key', total: 4,
      per_account: [{account_id: 3, email: 'me@example.test', count: 4}],
    })),
    saveSenderPolicy: record('saveSenderPolicy', {id: 11}),
    startSenderPolicySweep: record('startSenderPolicySweep', [{queued: 4, remaining: 0}]),
    previewSenderSweep: record('previewSenderSweep', input => ({
      address: input.address, mode: input.mode, snapshot_key: `sweep-${input.mode}`,
      total: 6, folders: [{folder_id: 1, name: 'Входящие', count: 6}],
    })),
    startSenderSweep: record('startSenderSweep', {state: 'completed', queued: 0}),
    updateSenderSweepMode: record('updateSenderSweepMode', null),
    setSenderSweepEnabled: record('setSenderSweepEnabled', null),
    deleteSenderSweepRule: record('deleteSenderSweepRule', null),
    deleteSenderPolicy: record('deleteSenderPolicy', {cancelled: 0, irreversible: 0}),
    previewIgnoreConversation: record('previewIgnoreConversation', {
      account_id: 3, account_email: 'me@example.test', subject: 'Тема',
      snapshot_key: 'ignore-key', total: 2, partial: false, existing_id: null,
    }),
    enableIgnoreConversation: record('enableIgnoreConversation', {id: 5}),
    disableIgnoreConversation: record('disableIgnoreConversation', {queued: 0, remaining: 2}),
    continueSenderPolicySweep: record('continueSenderPolicySweep', null),
    cancelSenderPolicySweep: record('cancelSenderPolicySweep', {irreversible: 0}),
    continueSenderSweepJob: record('continueSenderSweepJob', null),
    cancelSenderSweepJob: record('cancelSenderSweepJob', {irreversible: 0}),
  };
  const context = vm.createContext(sandbox);
  vm.runInContext(readModule('sender-lists.js'), context);
  vm.runInContext(readModule('sender-actions.js'), context);
  return {sandbox, hosts, body, calls};
}

// Дать выполниться уже начатым обещаниям обработчика: путь интерфейса
// асинхронный, и без ожидания диалог ещё не готов.
const settle = async (rounds = 20) => {
  for (let index = 0; index < rounds; index += 1) {
    await new Promise(resolve => setTimeout(resolve, 0));
  }
};

const menuClick = (host, action) => {
  const button = new FakeNode('button');
  button.dataset.threadAction = action;
  host.dispatch('click', {target: button});
};

test('S-027 - S-031: пункт меню открывает блокировку отправителя и уборку идёт через мост команд', async () => {
  const ui = startUi();
  menuClick(ui.hosts.get('threadMoreMenu'), 'block-sender');
  await settle();
  const overlay = ui.body.children.at(-1);
  assert.ok(overlay, 'диалог блокировки открылся из пункта меню');
  // S-029: число уже полученных писем показано до подтверждения.
  assert.match(overlay.querySelector('.sender-counts').textContent, /4/);
  // S-027: предложены адрес и домен, выбран адрес.
  assert.deepEqual(
    overlay.querySelectorAll('option').map(option => option.value),
    ['address', 'domain'],
  );
  assert.equal(overlay.querySelector('.sender-kind').value, 'address');
  // S-030: уборка полученных писем включается отдельным согласием.
  const consent = overlay.querySelector('.sender-sweep-consent');
  assert.equal(consent.checked, false, 'переключатель уборки выключен по умолчанию');
  consent.checked = true;
  await overlay.querySelector('.sender-apply').onclick();
  const names = ui.calls.map(call => call.name);
  assert.ok(names.includes('saveSenderPolicy'), 'запись сохраняется через мост команд');
  const sweep = ui.calls.find(call => call.name === 'startSenderPolicySweep');
  assert.ok(sweep, 'по согласию запускается уборка уже полученных писем');
  assert.deepEqual(sweep.args, [11, 'policy-key', true], 'уборка идёт по ключу снимка');
});

test('S-010, S-011: режим "новые сразу" идёт общим путём подтверждения с числом писем и ключом снимка', async () => {
  const ui = startUi();
  menuClick(ui.hosts.get('threadMoreMenu'), 'sweep-sender');
  await settle();
  const overlay = ui.body.children.at(-1);
  const mode = overlay.querySelector('.sweep-mode');
  mode.value = 'new_now';
  await mode.onchange();
  await settle();
  assert.match(
    overlay.querySelector('.sweep-counts').textContent,
    /6/,
    'число уже полученных писем показано и для режима "новые сразу"',
  );
  await overlay.querySelector('.sender-apply').onclick();
  const start = ui.calls.find(call => call.name === 'startSenderSweep');
  assert.ok(start, 'уборка запускается через мост команд');
  assert.equal(start.args[1], 'sweep-new_now', 'ключ снимка уходит в ядро и для этого режима');
});

test('S-031, S-028: список переписок показывает участников и дату, а режим автоочистки правится на месте', async () => {
  const ui = startUi({
    conversations: [{
      id: 5, account_email: 'me@example.test', subject: 'Счёт',
      participants: 'boss@example.test, me@example.test', state: 'enabled',
      partial: false, created_at: '2026-09-01 10:00', moved: 3, returned: 0,
      skipped: 0, failed: 0, queue_failed: 2, queue_error: 'сервер отказал',
    }],
    sweepRules: [{
      id: 9, address: 'shop@example.test', account_id: null, mode: 'only_last',
      days: null, sweep_archive: false, enabled: true, queued: 2, failed: 0,
    }],
  });
  await ui.sandbox.reloadSenderSections();
  const listed = ui.hosts.get('ignoredConversationsList').text();
  assert.match(listed, /boss@example\.test/, 'участники видны в списке');
  assert.match(listed, /2026-09-01/, 'дата включения видна в списке');
  // S-049: поздний отказ очереди виден там же.
  assert.match(listed, /отказов очереди: 2/);

  const editButton = ui.hosts.get('senderSweepList').descendants()
    .find(node => node.textContent === 'Изменить');
  assert.ok(editButton, 'правка записи автоочистки доступна из списка');
  editButton.onclick();
  await settle(2);
  const overlay = ui.body.children.at(-1);
  overlay.querySelector('.sweep-mode').value = 'older_than';
  overlay.querySelector('.sweep-days-value').value = '45';
  overlay.querySelector('.sweep-archive').checked = true;
  await overlay.querySelector('.sender-apply').onclick();
  const update = ui.calls.find(call => call.name === 'updateSenderSweepMode');
  assert.ok(update, 'правка уходит одной командой ядра');
  assert.deepEqual(update.args, [9, 'older_than', 45, true]);
});

test('S-051, S-052: смена языка перерисовывает разделы вместе с блоком незавершённых уборок', async () => {
  const ui = startUi({
    policies: [{
      id: 1, kind: 'domain', value: 'spam.test', decision: 'blocked',
      created_at: '2026-09-01', updated_at: '2026-09-01', swept: 3,
    }],
    conversations: [{
      id: 5, account_email: 'me@example.test', subject: 'Счёт', participants: 'boss@example.test',
      state: 'enabled', partial: false, created_at: '2026-09-01', moved: 1,
      returned: 0, skipped: 0, failed: 0,
    }],
    policyJobs: [{id: 4, state: 'pending', queued: 10, skipped: 0, failed: 0, remaining: 7}],
  });
  await ui.sandbox.reloadSenderSections();
  assert.match(ui.hosts.get('senderPendingJobs').text(), /осталось 7/);
  assert.match(ui.hosts.get('ignoredConversationsList').text(), /Игнорируется/);

  ui.sandbox.English = true;
  await ui.sandbox.relocalizeSenderSections();
  assert.match(
    ui.hosts.get('senderPendingJobs').text(),
    /left 7/,
    'блок незавершённых уборок переведён вместе с разделами',
  );
  assert.match(ui.hosts.get('ignoredConversationsList').text(), /Ignored/);
  assert.match(ui.hosts.get('senderPoliciesList').text(), /Blocked/);
});

test('S-007, S-008, S-013: разбор адреса отправителя отбрасывает непригодные значения', () => {
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

test('S-034, S-038: отчёт о возврате называет число непрошедших возврат писем', () => {
  const report = {queued: 3, skipped: 2, failed: 1, irreversible: 4, remaining: 5};
  assert.match(senders.returnReportText(report, 'ru'), /не найдено в корзине 2/);
  assert.match(senders.returnReportText(report, 'en'), /not found in trash 2/);
  assert.match(senders.sweepReportText(report, 'ru'), /осталось 5/);
  assert.match(senders.sweepReportText(report, 'en'), /left 5/);
});
