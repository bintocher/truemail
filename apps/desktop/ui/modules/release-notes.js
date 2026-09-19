// truemail UI module: release-notes.js
// Чистый разбор описания выпуска в перечень узлов: заголовки разделов, пункты
// списка и обычные абзацы. Без DOM и Tauri API, чтобы разбор проверялся сам по
// себе. Подключается в index.html перед window-chrome.js как обычный скрипт.
// Задача: issue #99.

// Описание приходит из манифеста обновления, то есть снаружи программы.
// Поэтому разбор не отдаёт разметку строкой: он возвращает перечень узлов с
// чистым текстом, а окно собирает их через создание элементов. Строка из сети
// в разметку не попадает ни при каких условиях.

// Заголовок раздела: от одной до шести решёток, дальше текст.
const HEADING = /^(#{1,6})\s+(.*)$/;
// Решётки без текста: в описании выпуска это мусор, а не заголовок.
const EMPTY_HEADING = /^#{1,6}\s*$/;
// Пункт списка: дефис, звёздочка или плюс с пробелом.
const BULLET = /^[-*+]\s+(.*)$/;
// Пункт нумерованного списка: число, точка или скобка, пробел.
const NUMBERED = /^(\d{1,3})[.)]\s+(.*)$/;

// Подчёркивания и звёздочки вокруг слова markdown показывает выделением, а не
// самими символами. Разбора выделения тут нет, поэтому парные символы просто
// снимаются: иначе пользователь видит их как мусор.
function stripEmphasis(text) {
  return text
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/__([^_]+)__/g, '$1')
    .replace(/(^|[\s(])\*([^*\n]+)\*(?=[\s).,;:!?]|$)/g, '$1$2')
    .replace(/`([^`]+)`/g, '$1');
}

// Разбор описания выпуска. Возвращает перечень узлов вида
// {kind: 'heading'|'item'|'text', text, level} - level только у заголовка.
function parseReleaseNotes(source) {
  const text = typeof source === 'string' ? source : '';
  const nodes = [];
  let paragraph = [];
  let last = null;

  // Абзац копится строками и закрывается пустой строкой или узлом другого
  // вида: в описании выпуска перенос внутри абзаца встречается часто.
  const flush = () => {
    if (!paragraph.length) return;
    nodes.push({kind: 'text', text: paragraph.join(' ')});
    last = nodes[nodes.length - 1];
    paragraph = [];
  };
  const push = node => {
    nodes.push(node);
    last = node;
  };

  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || EMPTY_HEADING.test(line)) {
      flush();
      last = null;
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      flush();
      const value = stripEmphasis(heading[2].trim());
      if (value) push({kind: 'heading', text: value, level: heading[1].length});
      continue;
    }
    const bullet = BULLET.exec(line);
    if (bullet) {
      flush();
      const value = stripEmphasis(bullet[1].trim());
      if (value) push({kind: 'item', text: value});
      continue;
    }
    const numbered = NUMBERED.exec(line);
    if (numbered) {
      flush();
      const value = stripEmphasis(numbered[2].trim());
      if (value) push({kind: 'item', text: `${numbered[1]}. ${value}`});
      continue;
    }
    // Длинный пункт часто переносится на следующую строку. Такое продолжение
    // дописывается к пункту: иначе хвост предложения уезжает из списка
    // отдельным абзацем, и список выглядит рваным.
    if (last && last.kind === 'item' && !paragraph.length) {
      last.text = `${last.text} ${stripEmphasis(line)}`;
      continue;
    }
    paragraph.push(stripEmphasis(line));
  }
  flush();
  return nodes;
}

const releaseNotes = {parseReleaseNotes, stripEmphasis};
if (typeof module !== 'undefined' && module.exports) module.exports = releaseNotes;
