// Мини-DOM для проверок интерфейса.
// Модули интерфейса подключаются в index.html обычными тегами script и рисуют
// разметку настоящими вызовами document.*; проверка, читающая только чистые
// функции, не замечает, что обработчик или сборка узла сломались. Здесь
// подделаны браузерные примитивы, а сами модули выполняются настоящие - на
// разметке из apps/desktop/ui/index.html.
// Поддержано ровно то, чем пользуются модули: поиск по селектору, дерево
// узлов, обработчики, выделение с вставкой разметки. Остального намеренно нет -
// заглушка, которую никто не вызывает, только скрывала бы отсутствие поведения.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const uiRoot = path.join(__dirname, '..', '..', 'ui');
const readUi = name => fs.readFileSync(path.join(uiRoot, name), 'utf8');
const locales = () => ({
  ru: JSON.parse(readUi('locales/ru.json')),
  en: JSON.parse(readUi('locales/en.json')),
});

// Теги без закрывающей части: их содержимое не в дереве, а в атрибутах.
const VOID_TAGS = new Set(['input', 'br', 'img', 'hr', 'meta', 'link', 'source', 'col']);
const camel = name => name.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());

// Узел текста. Отдельный вид узла нужен потому, что порядок текста и
// элементов внутри родителя значим: вставка картинки по курсору обязана
// разрезать текст, а не дописать картинку в конец.
class FakeText {
  constructor(data) {
    this.nodeType = 3;
    this.data = String(data ?? '');
    this.parent = null;
  }

  get textContent() {
    return this.data;
  }

  set textContent(value) {
    this.data = String(value ?? '');
  }

  get nodeValue() {
    return this.data;
  }

  remove() {
    if (!this.parent) return;
    this.parent.childNodes = this.parent.childNodes.filter(node => node !== this);
    this.parent = null;
  }

  cloneNode() {
    return new FakeText(this.data);
  }

  descendants() {
    return [];
  }
}

// Разбор одной части селектора: тег, классы, #id и [attr] либо [attr="value"].
function parseSimple(part) {
  const spec = {tag: null, classes: [], id: null, attrs: [], not: [], pseudo: []};
  const attrPattern = /\[([\w-]+)(?:([~^$*|]?=)"?([^\]"]*)"?)?\]/g;
  let match;
  let rest = part;
  // Отрицание в селекторе: интерфейс отличает им семейства меню
  // (.att-menu:not(.flag-menu)), и без него реестр меню находит чужое.
  const notPattern = /:not\(([^)]*)\)/g;
  let notMatch;
  while ((notMatch = notPattern.exec(part)) !== null) spec.not.push(notMatch[1]);
  rest = rest.replace(notPattern, '');
  part = rest;
  while ((match = attrPattern.exec(part)) !== null) spec.attrs.push({name: match[1], op: match[2] || null, value: match[3] ?? null});
  rest = rest.replace(attrPattern, '');
  const head = rest.match(/^[\w-]+/);
  if (head) spec.tag = head[0].toLowerCase();
  rest = rest.slice(head ? head[0].length : 0);
  const idMatch = rest.match(/#([\w-]+)/);
  if (idMatch) spec.id = idMatch[1];
  rest = rest.replace(/#[\w-]+/g, '');
  // Состояния узла в селекторе: интерфейс ищет ими отмеченные переключатели
  // настроек и последний элемент перечня. Незнакомое состояние оставляем
  // несовпадением - лучше промолчать, чем совпасть неверно и дать проверке
  // ложную зелёную.
  const pseudoPattern = /::?([\w-]+)/g;
  let pseudo;
  while ((pseudo = pseudoPattern.exec(rest)) !== null) spec.pseudo.push(pseudo[1]);
  rest = rest.replace(pseudoPattern, '');
  spec.classes = rest.split('.').filter(Boolean);
  return spec;
}

// Состояние узла, которое браузер проверяет псевдоклассом.
function matchesState(node, name) {
  const siblings = node.parent ? node.parent.children : [];
  switch (name) {
    case 'checked': return Boolean(node.checked);
    case 'disabled': return Boolean(node.disabled);
    case 'enabled': return !node.disabled;
    case 'focus': return Boolean(node.focused);
    case 'first-child': return siblings[0] === node;
    case 'last-child': return siblings.at(-1) === node;
    case 'only-child': return siblings.length === 1 && siblings[0] === node;
    case 'empty': return node.childNodes.length === 0;
    case 'root': return !node.parent;
    default: return false;
  }
}

function matchesSimple(node, spec) {
  if (spec.tag && spec.tag !== '*' && node.tag !== spec.tag) return false;
  if (spec.not && spec.not.some(piece => matchesSimple(node, parseSimple(piece)))) return false;
  if (spec.pseudo && !spec.pseudo.every(name => matchesState(node, name))) return false;
  if (spec.id && node.attributes.id !== spec.id) return false;
  if (!spec.classes.every(name => node.classes.has(name))) return false;
  return spec.attrs.every(attr => {
    const value = attr.name.startsWith('data-') ? node.dataset[camel(attr.name.slice(5))] : node.attributes[attr.name];
    if (attr.value === null) return value !== undefined && value !== null;
    return String(value ?? '') === attr.value;
  });
}

// Селектор: перечисление через запятую, внутри - цепочка предков через пробел
// или через '>'. Этого набора хватает модулям интерфейса.
function matchesSelector(node, selector, scope) {
  return String(selector).split(',').map(part => part.trim()).filter(Boolean).some(part => {
    const pieces = part.split(/\s+/).filter(Boolean);
    let current = node;
    for (let index = pieces.length - 1; index >= 0; index -= 1) {
      const piece = pieces[index];
      if (piece === '>') {
        const previous = pieces[index - 1];
        index -= 1;
        if (previous === ':scope') {
          if (current.parent !== scope) return false;
          continue;
        }
        current = current.parent;
        if (!current || !matchesSimple(current, parseSimple(previous))) return false;
        continue;
      }
      if (piece === ':scope') {
        if (current !== scope) return false;
        continue;
      }
      if (index === pieces.length - 1) {
        if (!matchesSimple(current, parseSimple(piece))) return false;
        continue;
      }
      // Предок на любом уровне: поднимаемся, пока не найдём подходящий.
      let ancestor = current.parent;
      while (ancestor && !matchesSimple(ancestor, parseSimple(piece))) ancestor = ancestor.parent;
      if (!ancestor) return false;
      current = ancestor;
    }
    return true;
  });
}

// Событие как его видят модули интерфейса: они сами создают Event и
// MouseEvent и читают поля кнопки и координат. Способы отмены держим в
// прототипе, а не в самом объекте: всплытие складывает свой набор полей и
// подменять его собственными способами события нельзя.
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
  constructor(tag) {
    this.nodeType = 1;
    this.tag = String(tag || 'div').toLowerCase();
    this.childNodes = [];
    this.classes = new Set();
    this.dataset = {};
    this.attributes = {};
    this.listeners = {};
    this.value = '';
    this.checked = false;
    this.disabled = false;
    this.parent = null;
    this.tabIndex = -1;
    this.focused = false;
    this.style = createStyle();
    this.options = [];
    // Размеры и прокрутка: список писем рисует только видимое окно строк и
    // считает его по высоте узла, а всплывающие меню по своей ширине решают,
    // раскрыться влево или вправо. Без размеров расчёт даёт NaN и строк в
    // списке не появляется вовсе.
    this.scrollTop = 0;
    this.scrollLeft = 0;
    this.scrollHeight = 0;
    this.scrollWidth = 0;
    this.clientHeight = 600;
    this.clientWidth = 800;
    this.offsetTop = 0;
    this.offsetLeft = 0;
    this.offsetHeight = 100;
    this.offsetWidth = 200;
  }

  get tagName() {
    return this.tag.toUpperCase();
  }

  get children() {
    return this.childNodes.filter(node => node.nodeType === 1);
  }

  get firstChild() {
    return this.childNodes[0] || null;
  }

  get lastChild() {
    return this.childNodes.at(-1) || null;
  }

  get firstElementChild() {
    return this.children[0] || null;
  }

  get parentElement() {
    return this.parent;
  }

  get parentNode() {
    return this.parent;
  }

  get id() {
    return this.attributes.id || '';
  }

  set id(value) {
    this.attributes.id = String(value);
  }

  // Подсказка и подпись поля - свойства узла и признаки разметки
  // одновременно: интерфейс пишет их то так, то так, а проверка читает одним
  // способом.
  get title() {
    return this.attributes.title ?? '';
  }

  set title(value) {
    this.attributes.title = String(value ?? '');
  }

  get placeholder() {
    return this.attributes.placeholder ?? '';
  }

  set placeholder(value) {
    this.attributes.placeholder = String(value ?? '');
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
      add: (...names) => names.forEach(name => classes.add(name)),
      remove: (...names) => names.forEach(name => classes.delete(name)),
      contains: name => classes.has(name),
      toggle: (name, force) => {
        const on = force === undefined ? !classes.has(name) : Boolean(force);
        if (on) classes.add(name); else classes.delete(name);
        return on;
      },
    };
  }

  get textContent() {
    return this.childNodes.map(node => node.textContent).join('');
  }

  set textContent(value) {
    this.childNodes.forEach(node => {
      node.parent = null;
    });
    this.childNodes = [];
    const text = String(value ?? '');
    if (text) this.appendChild(new FakeText(text));
  }

  get innerText() {
    return this.textContent;
  }

  get innerHTML() {
    return serialize(this);
  }

  set innerHTML(html) {
    this.childNodes.forEach(node => {
      node.parent = null;
    });
    this.childNodes = [];
    parseMarkup(String(html ?? '')).forEach(node => this.appendChild(node));
  }

  appendChild(node) {
    if (node && (node.isFragment || node.nodeType === 11)) {
      [...node.childNodes].forEach(child => this.appendChild(child));
      return node;
    }
    if (node.parent) node.remove();
    node.parent = this;
    this.childNodes.push(node);
    if (this.tag === 'select' && node.tag === 'option') {
      this.options.push(node);
      if (!this.value) this.value = node.value;
    }
    return node;
  }

  append(...nodes) {
    nodes.forEach(node => this.appendChild(typeof node === 'string' ? new FakeText(node) : node));
  }

  insertBefore(node, reference) {
    if (!reference) return this.appendChild(node);
    if (node && (node.isFragment || node.nodeType === 11)) {
      [...node.childNodes].forEach(child => this.insertBefore(child, reference));
      return node;
    }
    const index = this.childNodes.indexOf(reference);
    if (index === -1) return this.appendChild(node);
    if (node.parent) node.remove();
    node.parent = this;
    this.childNodes.splice(index, 0, node);
    return node;
  }

  before(...nodes) {
    if (!this.parent) return;
    nodes.forEach(node => this.parent.insertBefore(typeof node === 'string' ? new FakeText(node) : node, this));
  }

  after(...nodes) {
    if (!this.parent) return;
    const next = this.parent.childNodes[this.parent.childNodes.indexOf(this) + 1] || null;
    nodes.forEach(node => this.parent.insertBefore(typeof node === 'string' ? new FakeText(node) : node, next));
  }

  replaceWith(node) {
    this.before(node);
    this.remove();
  }

  insertAdjacentElement(where, node) {
    if (where === 'beforebegin') this.before(node);
    else if (where === 'afterend') this.after(node);
    else if (where === 'afterbegin') this.insertBefore(node, this.firstChild);
    else this.appendChild(node);
    return node;
  }

  replaceChildren(...nodes) {
    this.childNodes.forEach(node => {
      node.parent = null;
    });
    this.childNodes = [];
    nodes.forEach(node => this.appendChild(node));
  }

  removeChild(node) {
    this.childNodes = this.childNodes.filter(child => child !== node);
    node.parent = null;
    return node;
  }

  remove() {
    if (!this.parent) return;
    this.parent.childNodes = this.parent.childNodes.filter(node => node !== this);
    this.parent = null;
  }

  cloneNode(deep) {
    const copy = new FakeNode(this.tag);
    copy.classes = new Set(this.classes);
    copy.dataset = {...this.dataset};
    copy.attributes = {...this.attributes};
    copy.value = this.value;
    copy.checked = this.checked;
    if (deep) this.childNodes.forEach(node => copy.appendChild(node.cloneNode(true)));
    return copy;
  }

  setAttribute(name, value) {
    if (name === 'class') {
      this.className = value;
      return;
    }
    if (name.startsWith('data-')) {
      this.dataset[camel(name.slice(5))] = String(value);
      return;
    }
    this.attributes[name] = String(value);
    if (name === 'value') this.value = String(value);
  }

  getAttribute(name) {
    if (name === 'class') return this.className;
    if (name.startsWith('data-')) return this.dataset[camel(name.slice(5))] ?? null;
    return this.attributes[name] ?? null;
  }

  removeAttribute(name) {
    delete this.attributes[name];
  }

  hasAttribute(name) {
    return this.attributes[name] !== undefined;
  }

  toggleAttribute(name, force) {
    const on = force === undefined ? !this.hasAttribute(name) : Boolean(force);
    if (on) this.attributes[name] = ''; else delete this.attributes[name];
    return on;
  }

  addEventListener(type, handler) {
    (this.listeners[type] = this.listeners[type] || []).push(handler);
  }

  removeEventListener(type, handler) {
    this.listeners[type] = (this.listeners[type] || []).filter(item => item !== handler);
  }

  // Событие идёт от узла вверх по дереву, как в браузере: обработчики
  // композера и списка висят на окне, а не на самой строке.
  dispatch(type, event = {}) {
    const payload = {
      type,
      target: this,
      preventDefault() {
        payload.defaultPrevented = true;
      },
      stopPropagation() {
        payload.propagationStopped = true;
      },
      defaultPrevented: false,
      propagationStopped: false,
      ...event,
    };
    let node = this;
    while (node) {
      (node.listeners[type] || []).forEach(handler => handler.call(node, payload));
      const inline = node[`on${type}`];
      if (typeof inline === 'function') inline.call(node, payload);
      if (payload.propagationStopped) break;
      node = node.parent;
    }
    return payload;
  }

  dispatchEvent(event) {
    const payload = this.dispatch(event.type, event);
    return !payload.defaultPrevented;
  }

  focus() {
    this.focused = true;
    const owner = documentOf(this);
    if (owner) owner.activeElement = this;
  }

  // Выделение набранного текста: интерфейс зовёт его, открывая поле поиска,
  // чтобы следующий ввод заменил прежний запрос.
  select() {
    this.selectionStart = 0;
    this.selectionEnd = String(this.value ?? '').length;
  }

  setSelectionRange(start, end) {
    this.selectionStart = start;
    this.selectionEnd = end;
  }

  blur() {
    this.focused = false;
    // Уход фокуса виден и документу: интерфейс решает по activeElement, кому
    // достался Escape - полю ввода или окну целиком.
    const owner = documentOf(this);
    if (owner && owner.activeElement === this) owner.activeElement = null;
  }

  click() {
    this.dispatch('click', {});
  }

  contains(node) {
    let current = node;
    while (current) {
      if (current === this) return true;
      current = current.parent;
    }
    return false;
  }

  closest(selector) {
    let node = this;
    while (node) {
      if (node.nodeType === 1 && matchesSelector(node, selector, node)) return node;
      node = node.parent;
    }
    return null;
  }

  matches(selector) {
    return matchesSelector(this, selector, this);
  }

  descendants() {
    return this.childNodes.flatMap(node => (node.nodeType === 1 ? [node, ...node.descendants()] : []));
  }

  querySelector(selector) {
    return this.querySelectorAll(selector)[0] || null;
  }

  querySelectorAll(selector) {
    return this.descendants().filter(node => matchesSelector(node, selector, this));
  }

  getBoundingClientRect() {
    return {
      top: this.offsetTop,
      left: this.offsetLeft,
      right: this.offsetLeft + this.offsetWidth,
      bottom: this.offsetTop + this.offsetHeight,
      width: this.offsetWidth,
      height: this.offsetHeight,
      x: this.offsetLeft,
      y: this.offsetTop,
    };
  }

  scrollIntoView() {}
}

function createStyle() {
  const values = {};
  return {
    values,
    setProperty(name, value) {
      values[name] = value;
    },
    getPropertyValue(name) {
      return values[name] ?? '';
    },
    removeProperty(name) {
      delete values[name];
    },
  };
}

function documentOf(node) {
  let current = node;
  while (current.parent) current = current.parent;
  return current.ownerDocument || null;
}

function serialize(node) {
  return node.childNodes.map(child => {
    if (child.nodeType === 3) return child.data;
    const attributes = [];
    if (child.classes.size) attributes.push(` class="${child.className}"`);
    Object.entries(child.attributes).forEach(([name, value]) => attributes.push(` ${name}="${value}"`));
    Object.entries(child.dataset).forEach(([name, value]) => attributes.push(` data-${name}="${value}"`));
    const open = `<${child.tag}${attributes.join('')}>`;
    if (VOID_TAGS.has(child.tag)) return open;
    return `${open}${serialize(child)}</${child.tag}>`;
  }).join('');
}

// Разбор разметки в дерево узлов. Нужны теги, атрибуты, классы, признаки data
// и текст между тегами: модули ищут свои узлы именно по ним, а текст строки
// списка и шапки письма проверяется как раз по текстовым узлам.
function parseMarkup(html) {
  const root = [];
  const stack = [];
  const push = node => {
    const parent = stack.at(-1);
    if (parent) parent.appendChild(node);
    else root.push(node);
  };
  const tagPattern = /<(\/?)([a-zA-Z][\w-]*)((?:"[^"]*"|'[^']*'|[^>])*?)(\/?)>/g;
  let position = 0;
  let match;
  while ((match = tagPattern.exec(html)) !== null) {
    if (match.index > position) {
      const text = html.slice(position, match.index);
      if (text) push(new FakeText(decodeEntities(text)));
    }
    position = tagPattern.lastIndex;
    const tag = match[2].toLowerCase();
    if (match[1] === '/') {
      const index = stack.map(node => node.tag).lastIndexOf(tag);
      if (index !== -1) stack.length = index;
      continue;
    }
    const node = new FakeNode(tag);
    const attributePattern = /([\w:-]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/g;
    let attribute;
    while ((attribute = attributePattern.exec(match[3] || '')) !== null) {
      const name = attribute[1];
      const raw = attribute[2] ?? attribute[3] ?? attribute[4];
      const value = raw === undefined ? '' : decodeEntities(raw);
      if (name === 'class') node.className = value;
      else if (name.startsWith('data-')) node.dataset[camel(name.slice(5))] = value;
      else node.attributes[name] = value;
      if (name === 'value') node.value = value;
      if (name === 'checked') node.checked = true;
      if (name === 'disabled') node.disabled = true;
    }
    push(node);
    if (!VOID_TAGS.has(tag) && match[4] !== '/') stack.push(node);
  }
  if (position < html.length) {
    const text = html.slice(position);
    if (text) push(new FakeText(decodeEntities(text)));
  }
  return root;
}

function decodeEntities(text) {
  return String(text)
    .replace(/&lt;/g, '<').replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"').replace(/&#39;/g, "'")
    .replace(/&times;/g, '×').replace(/&nbsp;/g, ' ')
    .replace(/&amp;/g, '&');
}

// Выделение и вставка разметки. Позиция - пара (узел, смещение): в текстовом
// узле это номер символа, в элементе - номер потомка. Без разреза текста
// картинка всегда падала бы в конец тела, и проверка "вставка по курсору" не
// отличала бы правильное поведение от неправильного.
class FakeRange {
  constructor(container, offset) {
    this.startContainer = container;
    this.startOffset = offset;
  }

  setStart(container, offset) {
    this.startContainer = container;
    this.startOffset = offset;
  }

  selectNodeContents(node) {
    this.startContainer = node;
    this.startOffset = node.childNodes.length;
  }

  collapse() {}

  cloneRange() {
    return new FakeRange(this.startContainer, this.startOffset);
  }

  // Вставка узлов на место курсора с разрезом текста. Возвращает позицию
  // сразу за последним вставленным узлом - следующая картинка той же вставки
  // встаёт за предыдущей, а не поверх неё.
  insertNodes(nodes) {
    let parent = this.startContainer;
    let index = this.startOffset;
    if (parent.nodeType === 3) {
      const text = parent;
      const owner = text.parent;
      const tail = text.data.slice(this.startOffset);
      text.data = text.data.slice(0, this.startOffset);
      index = owner.childNodes.indexOf(text) + 1;
      if (tail) {
        const rest = new FakeText(tail);
        rest.parent = owner;
        owner.childNodes.splice(index, 0, rest);
      }
      parent = owner;
    }
    nodes.forEach((node, shift) => {
      if (node.parent) node.remove();
      node.parent = parent;
      parent.childNodes.splice(index + shift, 0, node);
    });
    this.startContainer = parent;
    this.startOffset = index + nodes.length;
    return this;
  }
}

// Файл в памяти - то, что приходит из буфера обмена и из данных переноса.
// Байты настоящие: по ним считается размер письма и собирается строка data:.
class FakeFile {
  constructor(name, type, bytesOrSize) {
    this.name = name;
    this.type = type;
    // Числом задаётся только заявленный размер, без тела: файл на десятки
    // мегабайт нужен проверке предела, а держать его байты в памяти незачем -
    // до чтения дело не доходит, отказ случается раньше.
    if (typeof bytesOrSize === 'number') {
      this.bytes = Buffer.alloc(0);
      this.size = bytesOrSize;
      return;
    }
    this.bytes = Buffer.from(bytesOrSize);
    this.size = this.bytes.length;
  }

  arrayBuffer() {
    return Promise.resolve(this.bytes);
  }
}

// Чтение файла в строку data: - тот же примитив, которым композер читает
// картинку из буфера. Чтение асинхронное, как в браузере: между началом
// чтения и вставкой письмо успевает смениться.
function createFileReaderClass() {
  return class FakeFileReader {
    constructor() {
      this.result = '';
      this.error = null;
      this.onload = null;
      this.onerror = null;
    }

    readAsDataURL(file) {
      setTimeout(() => {
        if (file && file.failRead) {
          this.error = new Error('read failed');
          if (this.onerror) this.onerror();
          return;
        }
        this.result = `data:${file.type};base64,${file.bytes.toString('base64')}`;
        if (this.onload) this.onload();
      }, 0);
    }
  };
}

function createSelection() {
  let range = null;
  return {
    get rangeCount() {
      return range ? 1 : 0;
    },
    get anchorNode() {
      return range ? range.startContainer : null;
    },
    getRangeAt() {
      return range;
    },
    removeAllRanges() {
      range = null;
    },
    addRange(value) {
      range = value;
    },
    current() {
      return range;
    },
  };
}



// Разобрать разметку в один узел-обёртку: разбор сам по себе отдаёт
// перечень корней, а поиск селектором нужен по всему куску сразу.
function parseHtml(html, ownerDocument = null) {
  const fragment = new FakeNode('#fragment');
  fragment.nodeType = 11;
  fragment.isFragment = true;
  if (ownerDocument) fragment.ownerDocument = ownerDocument;
  parseMarkup(String(html ?? '')).forEach(node => fragment.appendChild(node));
  return fragment;
}

module.exports = {
  FakeNode,
  createSelection,
  createFileReaderClass,
  FakeEvent,
  FakeText,
  FakeRange,
  FakeFile,
  parseMarkup,
  parseHtml,
  matchesSelector,
  readUi,
  locales,
  uiRoot,
};
