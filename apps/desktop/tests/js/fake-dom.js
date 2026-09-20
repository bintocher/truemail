// Мини-дерево страницы для проверок интерфейса. Нужен, чтобы модули окна
// выполнялись целиком на настоящей разметке из index.html, а не проверялись
// как чистые функции в отрыве от окна: почти все дефекты интерфейса жили
// именно в связке "разметка - обработчик - мост", и чистые проверки их не
// видели. Узлы, селекторы и всплытие событий повторяют поведение браузера
// настолько, насколько это нужно проверяемым путям.
// Разметка разбирается из настоящего index.html, поэтому переименование класса
// или признака data в окне обязано ронять проверки, которые по ним ищут.

'use strict';

const VOID_TAGS = new Set(['input', 'br', 'img', 'hr', 'meta', 'link', 'source', 'area', 'base', 'col']);
const camel = name => name.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());

// Разбор одного составного селектора: тег, классы, идентификатор, признаки
// data и :not(). Псевдоклассы показа (:hover и прочие) в проверках не нужны и
// считаются несовпадением - лучше промолчать, чем совпасть неверно.
function parseCompound(text) {
  const part = {tag: null, classes: [], id: null, attributes: [], not: [], scope: false};
  const pattern = /([.#]?[\w-]+|\[[^\]]+\]|:not\(([^)]+)\)|:scope|\*|::?[\w-]+(?:\([^)]*\))?)/g;
  let match;
  while ((match = pattern.exec(text)) !== null) {
    const token = match[1];
    if (token === '*') continue;
    if (token === ':scope') { part.scope = true; continue; }
    if (token.startsWith(':not(')) { part.not.push(parseCompound(match[2])); continue; }
    if (token.startsWith('::') || token.startsWith(':')) { part.unsupported = true; continue; }
    if (token.startsWith('.')) { part.classes.push(token.slice(1)); continue; }
    if (token.startsWith('#')) { part.id = token.slice(1); continue; }
    if (token.startsWith('[')) {
      const attribute = token.slice(1, -1).match(/^([\w-]+)(?:([~^$*|]?=)"?([^"\]]*)"?)?$/);
      if (attribute) part.attributes.push({name: attribute[1], operator: attribute[2] || null, value: attribute[3] ?? null});
      continue;
    }
    part.tag = token.toLowerCase();
  }
  return part;
}

function parseSelector(selector) {
  return String(selector).split(',').map(piece => {
    const steps = [];
    const tokens = piece.trim().split(/\s*(>)\s*|\s+/).filter(token => token !== undefined && token !== '');
    let combinator = null;
    tokens.forEach(token => {
      if (token === '>') { combinator = 'child'; return; }
      steps.push({compound: parseCompound(token), combinator});
      combinator = 'descendant';
    });
    return steps;
  });
}

function attributeValue(node, name) {
  if (name.startsWith('data-')) {
    const key = camel(name.slice(5));
    return node.dataset[key] === undefined ? null : node.dataset[key];
  }
  if (name === 'class') return node.className;
  if (name === 'id') return node.attributes.id ?? null;
  return node.attributes[name] === undefined ? null : node.attributes[name];
}

function matchesCompound(node, part, scopeNode) {
  if (part.unsupported) return false;
  if (part.scope && node !== scopeNode) return false;
  if (part.tag && node.tag !== part.tag) return false;
  if (part.id && node.attributes.id !== part.id) return false;
  if (part.classes.some(name => !node.classes.has(name))) return false;
  const attributesOk = part.attributes.every(attribute => {
    const value = attributeValue(node, attribute.name);
    if (value === null) return false;
    if (attribute.operator === null) return true;
    if (attribute.operator === '=') return value === attribute.value;
    if (attribute.operator === '~=') return String(value).split(/\s+/).includes(attribute.value);
    if (attribute.operator === '^=') return String(value).startsWith(attribute.value);
    if (attribute.operator === '$=') return String(value).endsWith(attribute.value);
    if (attribute.operator === '*=') return String(value).includes(attribute.value);
    return false;
  });
  if (!attributesOk) return false;
  return !part.not.some(inner => matchesCompound(node, inner, scopeNode));
}

function matchesSteps(node, steps, scopeNode) {
  const last = steps[steps.length - 1];
  if (!matchesCompound(node, last.compound, scopeNode)) return false;
  let index = steps.length - 2;
  let current = node;
  let combinator = last.combinator;
  while (index >= 0) {
    const step = steps[index];
    if (combinator === 'child') {
      current = current.parentElement;
      if (!current || !matchesCompound(current, step.compound, scopeNode)) return false;
    } else {
      let ancestor = current.parentElement;
      while (ancestor && !matchesCompound(ancestor, step.compound, scopeNode)) ancestor = ancestor.parentElement;
      if (!ancestor) return false;
      current = ancestor;
    }
    combinator = step.combinator;
    index -= 1;
  }
  return true;
}

function matches(node, selector, scopeNode = null) {
  if (!node || node.nodeType !== 1) return false;
  return parseSelector(selector).some(steps => steps.length && matchesSteps(node, steps, scopeNode));
}

class FakeEvent {
  constructor(type, init = {}) {
    Object.assign(this, {button: 0, bubbles: true, clientX: 0, clientY: 0}, init);
    this.type = type;
    this.defaultPrevented = false;
    this.propagationStopped = false;
    this.immediateStopped = false;
  }

  preventDefault() { this.defaultPrevented = true; }

  stopPropagation() { this.propagationStopped = true; }

  stopImmediatePropagation() { this.propagationStopped = true; this.immediateStopped = true; }
}

class FakeNode {
  constructor(tag, ownerDocument = null) {
    this.tag = String(tag || 'div').toLowerCase();
    this.nodeType = this.tag === '#fragment' ? 11 : 1;
    this.ownerDocument = ownerDocument;
    this.children = [];
    this.classes = new Set();
    // Значения признаков data всегда строки, как в браузере: код окна пишет
    // туда и числа (номер письма), а ищет по строке в селекторе.
    this.dataset = new Proxy({}, {set(target, key, value) { target[key] = String(value); return true; }});
    this.attributes = {};
    this.listeners = new Map();
    this.text = '';
    this.value = '';
    this.parentElement = null;
    this.hidden = false;
    this.title = '';
    this.tabIndex = -1;
    this.draggable = false;
    this.disabled = false;
    this.checked = false;
    this.scrollTop = 0;
    this.scrollHeight = 0;
    this.clientHeight = 600;
    this.offsetWidth = 200;
    this.offsetHeight = 100;
    this.style = createStyle();
  }

  get parentNode() { return this.parentElement; }

  get childNodes() { return this.children; }

  get firstElementChild() { return this.children[0] || null; }

  get lastElementChild() { return this.children[this.children.length - 1] || null; }

  // Пункты списка выбора: окно перебирает select.options, проверяя, есть ли
  // нужный ящик среди отправителей. Без этого свойства возврат письма в
  // композер падает на выборе ящика и проверка не доходит до сути.
  get options() { return this.children.filter(child => child.tag === 'option'); }

  get nextElementSibling() {
    const siblings = this.parentElement ? this.parentElement.children : [];
    return siblings[siblings.indexOf(this) + 1] || null;
  }

  get previousElementSibling() {
    const siblings = this.parentElement ? this.parentElement.children : [];
    return siblings[siblings.indexOf(this) - 1] || null;
  }

  get classList() {
    return {
      add: (...names) => names.forEach(name => this.classes.add(name)),
      remove: (...names) => names.forEach(name => this.classes.delete(name)),
      contains: name => this.classes.has(name),
      toggle: (name, on) => {
        const wanted = on === undefined ? !this.classes.has(name) : Boolean(on);
        if (wanted) this.classes.add(name); else this.classes.delete(name);
        return wanted;
      },
    };
  }

  set className(value) { this.classes = new Set(String(value || '').split(/\s+/).filter(Boolean)); }

  get className() { return [...this.classes].join(' '); }

  // Идентификатор узла свойством, а не только атрибутом: переключение
  // представлений (showView в shell.js) сравнивает именно node.id, и без этого
  // свойства ни одно представление окна не становится активным.
  set id(value) { this.attributes.id = String(value); }

  get id() { return this.attributes.id ?? ''; }

  set textContent(value) {
    this.children.forEach(child => { child.parentElement = null; });
    this.children = [];
    this.text = value === null || value === undefined ? '' : String(value);
  }

  get textContent() {
    return this.text + this.children.map(child => child.textContent).join('');
  }

  get innerText() { return this.textContent; }

  set innerText(value) { this.textContent = value; }

  set innerHTML(value) {
    this.children.forEach(child => { child.parentElement = null; });
    this.children = [];
    this.text = '';
    const parsed = parseHtml(String(value ?? ''), this.ownerDocument);
    this.text = parsed.text;
    // Перенос идёт по копии перечня: appendChild вынимает узел из прежнего
    // родителя, и обход самого перечня пропускал бы каждый второй узел.
    [...parsed.children].forEach(child => this.appendChild(child));
  }

  get innerHTML() {
    return this.text + this.children.map(child => child.outerHTML).join('');
  }

  get outerHTML() {
    const attributes = [];
    if (this.classes.size) attributes.push(`class="${this.className}"`);
    Object.keys(this.attributes).forEach(name => attributes.push(`${name}="${this.attributes[name]}"`));
    Object.keys(this.dataset).forEach(key => attributes.push(`data-${key.replace(/[A-Z]/g, letter => `-${letter.toLowerCase()}`)}="${this.dataset[key]}"`));
    const head = `<${this.tag}${attributes.length ? ` ${attributes.join(' ')}` : ''}>`;
    if (VOID_TAGS.has(this.tag)) return head;
    return `${head}${this.innerHTML}</${this.tag}>`;
  }

  appendChild(node) {
    if (!node) return node;
    if (node.nodeType === 11) {
      [...node.children].forEach(child => this.appendChild(child));
      node.children = [];
      return node;
    }
    if (node.parentElement) node.parentElement.removeChild(node);
    node.parentElement = this;
    node.ownerDocument = node.ownerDocument || this.ownerDocument;
    this.children.push(node);
    return node;
  }

  append(...nodes) { nodes.forEach(node => this.appendChild(typeof node === 'string' ? textNode(node, this.ownerDocument) : node)); }

  insertBefore(node, reference) {
    if (!reference) return this.appendChild(node);
    const index = this.children.indexOf(reference);
    if (index < 0) return this.appendChild(node);
    if (node.parentElement) node.parentElement.removeChild(node);
    node.parentElement = this;
    this.children.splice(index, 0, node);
    return node;
  }

  before(...nodes) {
    if (!this.parentElement) return;
    nodes.forEach(node => this.parentElement.insertBefore(typeof node === 'string' ? textNode(node, this.ownerDocument) : node, this));
  }

  after(...nodes) {
    if (!this.parentElement) return;
    const next = this.nextElementSibling;
    nodes.forEach(node => this.parentElement.insertBefore(typeof node === 'string' ? textNode(node, this.ownerDocument) : node, next));
  }

  replaceWith(node) {
    if (!this.parentElement) return;
    this.parentElement.insertBefore(node, this);
    this.remove();
  }

  removeChild(node) {
    const index = this.children.indexOf(node);
    if (index >= 0) this.children.splice(index, 1);
    node.parentElement = null;
    return node;
  }

  replaceChildren(...nodes) {
    this.children.forEach(child => { child.parentElement = null; });
    this.children = [];
    this.text = '';
    nodes.forEach(node => this.appendChild(node));
  }

  remove() { if (this.parentElement) this.parentElement.removeChild(this); }

  setAttribute(name, value) {
    if (name.startsWith('data-')) { this.dataset[camel(name.slice(5))] = String(value); return; }
    if (name === 'class') { this.className = value; return; }
    this.attributes[name] = String(value);
  }

  getAttribute(name) { return attributeValue(this, name); }

  hasAttribute(name) { return attributeValue(this, name) !== null; }

  removeAttribute(name) {
    if (name.startsWith('data-')) { delete this.dataset[camel(name.slice(5))]; return; }
    if (name === 'class') { this.classes = new Set(); return; }
    delete this.attributes[name];
  }

  toggleAttribute(name, force) {
    const wanted = force === undefined ? !this.hasAttribute(name) : Boolean(force);
    if (wanted) this.setAttribute(name, ''); else this.removeAttribute(name);
    return wanted;
  }

  addEventListener(type, handler) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(handler);
  }

  removeEventListener(type, handler) {
    const list = this.listeners.get(type) || [];
    const index = list.indexOf(handler);
    if (index >= 0) list.splice(index, 1);
  }

  // Событие идёт от узла вверх по дереву, как в браузере: обработчики на
  // документе (именно там живут контекстное меню, закрытие меню и конец
  // удержания списка) обязаны получать его с настоящим target.
  dispatchEvent(event) {
    event.target = event.target || this;
    let node = this;
    while (node) {
      event.currentTarget = node;
      const inline = node[`on${event.type}`];
      if (typeof inline === 'function') inline.call(node, event);
      const handlers = [...(node.listeners.get(event.type) || [])];
      for (const handler of handlers) {
        handler.call(node, event);
        if (event.immediateStopped) break;
      }
      if (event.propagationStopped || event.bubbles === false) break;
      node = node.parentElement || (node.ownerDocument && node !== node.ownerDocument ? node.ownerDocument : null);
    }
    return !event.defaultPrevented;
  }

  dispatch(type, init = {}) { return this.dispatchEvent(new FakeEvent(type, init)); }

  click() { return this.dispatch('click'); }

  focus() { if (this.ownerDocument) this.ownerDocument.activeElement = this; }

  blur() { if (this.ownerDocument && this.ownerDocument.activeElement === this) this.ownerDocument.activeElement = null; }

  // Выделение текста поля: окно зовёт его сразу за focus (кнопка фильтра
  // списка), и без метода обработчик падал бы на полпути.
  select() {}

  scrollIntoView() {}

  getBoundingClientRect() { return {top: 0, left: 0, right: this.offsetWidth, bottom: this.offsetHeight, width: this.offsetWidth, height: this.offsetHeight}; }

  descendants() { return this.children.flatMap(child => [child, ...child.descendants()]); }

  querySelector(selector) { return this.descendants().find(node => matches(node, selector, this)) || null; }

  querySelectorAll(selector) { return this.descendants().filter(node => matches(node, selector, this)); }

  matches(selector) { return matches(this, selector, this); }

  closest(selector) {
    let node = this;
    while (node && node.nodeType === 1) {
      if (matches(node, selector, node)) return node;
      node = node.parentElement;
    }
    return null;
  }

  contains(node) {
    let current = node;
    while (current) {
      if (current === this) return true;
      current = current.parentElement;
    }
    return false;
  }

  cloneNode(deep) {
    const copy = new FakeNode(this.tag, this.ownerDocument);
    copy.classes = new Set(this.classes);
    copy.dataset = {...this.dataset};
    copy.attributes = {...this.attributes};
    copy.text = this.text;
    copy.value = this.value;
    if (deep) this.children.forEach(child => copy.appendChild(child.cloneNode(true)));
    return copy;
  }
}

function createStyle() {
  const style = {
    properties: {},
    setProperty(name, value) { style.properties[name] = String(value); },
    getPropertyValue(name) { return style.properties[name] ?? ''; },
    removeProperty(name) { delete style.properties[name]; },
  };
  return style;
}

function textNode(value, ownerDocument) {
  const node = new FakeNode('span', ownerDocument);
  node.text = String(value);
  return node;
}

// Разбор разметки. Комментарии и содержимое script/style пропускаются: в
// проверках нужны только узлы, по которым ищут модули окна.
function parseHtml(html, ownerDocument = null) {
  const root = new FakeNode('#fragment', ownerDocument);
  const stack = [root];
  const source = String(html).replace(/<!--[\s\S]*?-->/g, '');
  const pattern = /<(\/?)([a-zA-Z][\w-]*)((?:"[^"]*"|'[^']*'|[^>])*?)(\/?)>/g;
  let lastIndex = 0;
  let match;
  const addText = value => {
    if (!value) return;
    const decoded = value.replace(/&nbsp;/g, ' ').replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"');
    if (!decoded.trim()) return;
    stack[stack.length - 1].text += decoded.trim();
  };
  while ((match = pattern.exec(source)) !== null) {
    addText(source.slice(lastIndex, match.index));
    lastIndex = pattern.lastIndex;
    const tag = match[2].toLowerCase();
    if (match[1] === '/') {
      if (stack.length > 1) stack.pop();
      continue;
    }
    if (tag === 'script' || tag === 'style') {
      const closing = new RegExp(`</${tag}\\s*>`, 'i');
      const rest = source.slice(pattern.lastIndex);
      const end = rest.search(closing);
      if (end >= 0) {
        pattern.lastIndex += end + rest.slice(end).match(closing)[0].length;
        lastIndex = pattern.lastIndex;
      }
      continue;
    }
    const node = new FakeNode(tag, ownerDocument);
    const attributePattern = /([\w:-]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g;
    let attribute;
    while ((attribute = attributePattern.exec(match[3] || '')) !== null) {
      const name = attribute[1];
      const value = attribute[2] ?? attribute[3] ?? attribute[4] ?? '';
      if (name === 'class') node.className = value;
      else if (name.startsWith('data-')) node.dataset[camel(name.slice(5))] = value;
      else node.attributes[name] = value;
      if (name === 'value') node.value = value;
      if (name === 'hidden') node.hidden = true;
      if (name === 'disabled') node.disabled = true;
    }
    stack[stack.length - 1].appendChild(node);
    if (!VOID_TAGS.has(tag) && match[4] !== '/') stack.push(node);
  }
  addText(source.slice(lastIndex));
  return root;
}

module.exports = {FakeNode, FakeEvent, parseHtml, matches, textNode, VOID_TAGS};
