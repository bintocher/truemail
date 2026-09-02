// truemail UI module: person-search.js
// Чистые функции без DOM и Tauri API: подбор людей по транслиту кириллица<->латиница
// плюс раскладко-независимый поиск (наследует matchQ). Единая точка транслитерации
// для подсказки адресата, палитры команд и раздела контактов. Подключается в
// index.html перед первым потребителем (commands-accessibility.js).
// См. docs/specs/person-search-translit.md.

// Таблица транслитерации русского алфавита (S-003). 'е' и 'ё' обрабатываются
// отдельно (S-005) - у них несколько допустимых написаний в зависимости от позиции.
const PERSON_TRANSLIT = {
  'а': 'a', 'б': 'b', 'в': 'v', 'г': 'g', 'д': 'd', 'ж': 'zh', 'з': 'z', 'и': 'i',
  'й': 'y', 'к': 'k', 'л': 'l', 'м': 'm', 'н': 'n', 'о': 'o', 'п': 'p', 'р': 'r',
  'с': 's', 'т': 't', 'у': 'u', 'ф': 'f', 'х': 'kh', 'ц': 'ts', 'ч': 'ch', 'ш': 'sh',
  'щ': 'shch', 'ъ': '', 'ь': '', 'ы': 'y', 'э': 'e', 'ю': 'yu', 'я': 'ya',
};
const PERSON_VOWELS = new Set(['а', 'е', 'ё', 'и', 'о', 'у', 'ы', 'э', 'ю', 'я']);
// Раскладка клавиатуры ЙЦУКЕН/QWERTY - своя копия таблицы: модуль загружается
// раньше commands-accessibility.js и не может использовать её RU/EN (S-001 запрещает
// только дублирование таблицы транслитерации, а не таблицы раскладки).
const PERSON_LAYOUT_RU = 'йцукенгшщзхъфывапролджэячсмитьбю';
const PERSON_LAYOUT_EN = "qwertyuiop[]asdfghjkl;'zxcvbnm,.";

function personConvLayout(value, from, to) {
  return value.split('').map(ch => { const i = from.indexOf(ch); return i >= 0 ? to[i] : ch; }).join('');
}

// 'е'/'ё' в начале слова и после гласной, 'й', 'ъ', 'ь' дополнительно дают
// написание с 'ye' (S-005): "начало слова" - индекс 0 либо предыдущий символ не буква.
function personIsYePosition(chars, index) {
  if (index === 0) return true;
  const prev = chars[index - 1];
  if (!/\p{L}/u.test(prev)) return true;
  return PERSON_VOWELS.has(prev) || prev === 'й' || prev === 'ъ' || prev === 'ь';
}

// Ветвление транслитерации по символам строки (уже в нижнем регистре). Не
// кириллические символы копируются как есть - латиница не транслитерируется
// (S-004: обратный разбор латиницы не требуется). Ветки капаются на 8 после
// каждого символа, чтобы не блистать комбинаторно.
function personTransliterateBranches(lower) {
  const chars = [...lower];
  let branches = [''];
  for (let i = 0; i < chars.length; i++) {
    const ch = chars[i];
    let additions;
    if (ch === 'е') additions = personIsYePosition(chars, i) ? ['e', 'ye'] : ['e'];
    else if (ch === 'ё') additions = personIsYePosition(chars, i) ? ['yo', 'e', 'ye'] : ['yo', 'e'];
    else additions = [Object.hasOwn(PERSON_TRANSLIT, ch) ? PERSON_TRANSLIT[ch] : ch];
    const next = [];
    for (const b of branches) for (const add of additions) next.push(b + add);
    branches = next.length > 8 ? next.slice(0, 8) : next;
  }
  return branches;
}

// Разложение Unicode, удаление диакритики и апострофов, схлопывание пробелов (S-007).
function personFinalize(value) {
  return value.normalize('NFD').replace(/[\u0300-\u036f]/g, '').replace(/['`\u2019]/g, '').replace(/\s+/g, ' ').trim();
}

function personDedupCapped(values, limit) {
  const seen = new Set(), out = [];
  for (const value of values) {
    if (!value || seen.has(value)) continue;
    seen.add(value); out.push(value);
    if (out.length >= limit) break;
  }
  return out;
}

// Поисковые ключи текста (S-001, S-005, S-007): нижний регистр текста как есть
// (для прямого и раскладочного сравнения) плюс его транслитерации в латиницу.
function personSearchKeys(text) {
  const lower = String(text || '').toLowerCase();
  if (!lower.trim()) return [];
  const plain = personFinalize(lower);
  const translit = personTransliterateBranches(lower).map(personFinalize);
  return personDedupCapped([plain, ...translit], 8);
}

// Варианты запроса (S-002, S-005, S-006, S-008): исходный запрос, две смены
// раскладки, затем транслитерация каждого из них - именно в этом порядке, чтобы
// "rjyyjdf" сначала стало "коннова", а потом "konnova".
function personSearchVariants(query) {
  const raw = String(query || '');
  if (!raw.trim()) return [];
  const lower = raw.toLowerCase();
  const base = personDedupCapped([lower, personConvLayout(lower, PERSON_LAYOUT_RU, PERSON_LAYOUT_EN), personConvLayout(lower, PERSON_LAYOUT_EN, PERSON_LAYOUT_RU)], 8);
  let pool = [...base];
  // Транслитерация включается с двух букв/цифр и не применяется к вводу адреса (S-008).
  const letterDigitCount = (raw.match(/[\p{L}\p{N}]/gu) || []).length;
  if (!raw.includes('@') && letterDigitCount >= 2) {
    for (const candidate of base) for (const branch of personTransliterateBranches(candidate)) pool.push(branch);
  }
  return personDedupCapped(pool.map(personFinalize), 8);
}

// Ключи текста матчатся с вариантами запроса поиском подстроки (S-006, S-009).
function personKeysMatchVariants(keys, variants) {
  return keys.some(key => variants.some(variant => key.includes(variant)));
}

// Итоговое сравнение (S-006, S-009): поиск подстроки между ключами текста и
// вариантами запроса. Пустой запрос ничего не отсеивает - как и раньше у matchQ.
function personMatches(text, query) {
  const q = String(query || '');
  if (!q.trim()) return true;
  const keys = personSearchKeys(text);
  if (!keys.length) return false;
  const variants = personSearchVariants(query);
  if (!variants.length) return false;
  return personKeysMatchVariants(keys, variants);
}

// Отбор подсказки адресата (S-013, S-014): пустой запрос - подсказка не
// показывается; иначе не более limit адресов, без уже выбранных (used,
// сравнение по email без учёта регистра) и без дублей внутри списка.
// keysFor(address) необязателен - через него поверхность передаёт свой кэш
// ключей (S-012), по умолчанию ключи считаются заново на каждый вызов.
function suggestRecipients(addresses, query, used, limit, keysFor) {
  const q = String(query || '');
  if (!q.trim()) return [];
  const variants = personSearchVariants(query);
  const usedLower = new Set([...(used || [])].map(email => String(email || '').toLowerCase()));
  const seen = new Set(), result = [];
  const keysForAddress = keysFor || (address => personSearchKeys(`${address?.name || ''} ${address?.email || ''}`));
  for (const address of addresses || []) {
    const email = String(address?.email || '').toLowerCase();
    if (!email || usedLower.has(email) || seen.has(email)) continue;
    const keys = keysForAddress(address);
    if (!keys.length || !variants.length || !personKeysMatchVariants(keys, variants)) continue;
    seen.add(email); result.push(address);
    if (result.length >= (limit ?? 8)) break;
  }
  return result;
}

// Кэш ключей набора данных (S-012): построение вызывается один раз на набор, а
// не на каждый ввод. Ключ кэша - id/адрес контакта, а не ссылка на объект: у
// подсказки адресата объекты пересоздаются на каждый рендер. invalidate()
// вызывается там же, где обновляется исходный набор данных (например coreContacts).
function createPersonSearchCache() {
  let map = new Map();
  return {
    get(id, textOrFactory) {
      if (map.has(id)) return map.get(id);
      const text = typeof textOrFactory === 'function' ? textOrFactory() : textOrFactory;
      const keys = personSearchKeys(text);
      map.set(id, keys);
      return keys;
    },
    invalidate() { map = new Map(); },
  };
}

const personSearch = { personSearchKeys, personSearchVariants, personMatches, suggestRecipients, createPersonSearchCache };
if (typeof module !== 'undefined' && module.exports) module.exports = personSearch;
