// Проверки картинок и файлов в письме: разбор данных буфера обмена и
// подсчёт размера (ui/modules/composer-body.js) и настоящий путь композера -
// вставка по курсору, отказ по типу и по пределу, брошенные файлы.
// Порядок "проверить предел - вставить - сдвинуть курсор" живёт в composer.js,
// а не в модели: проверка, повторяющая этот цикл у себя, зелена и тогда, когда
// в программе он собран неправильно.
// Спецификация: specs/composer-image-paste-and-file-drop.md.
// Запуск: node --test apps/desktop/tests/js/composer-body.test.js (Node 22+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {
  SUPPORTED_IMAGE_TYPES, MAX_MESSAGE_BYTES, isSupportedImageType, isFileTransfer,
  clipboardImageItems, parseDataUrl, buildImageTag, htmlHasImageTag, totalMessageBytes,
} = require('../../ui/modules/composer-body.js');
const {startApp} = require('./ui-window.js');
const {FakeFile} = require('./fake-dom.js');

const base64 = text => Buffer.from(text).toString('base64');
const dataUrl = (type, text) => `data:${type};base64,${base64(text)}`;

// --- Модель: отбор, разбор, подсчёт ---

// S-004, S-035: перечень поддерживаемых типов задан один раз. svg+xml в него
// не входит намеренно - это разметка, которая может нести исполняемый код.
test('S-004, S-035: поддерживаемые типы картинок перечислены одним списком', () => {
  assert.deepEqual(SUPPORTED_IMAGE_TYPES, ['image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/bmp']);
  SUPPORTED_IMAGE_TYPES.forEach(type => {
    assert.equal(isSupportedImageType(type), true, type);
    assert.equal(isSupportedImageType(type.toUpperCase()), true, `${type} в верхнем регистре`);
  });
  assert.equal(isSupportedImageType('image/svg+xml'), false);
  assert.equal(isSupportedImageType('text/plain'), false);
  assert.equal(isSupportedImageType(''), false);
});

// S-001, S-003: из буфера берутся только файловые картинки, в порядке буфера.
// Текст и разметка того же буфера вставляться картинкой не должны, и элемент
// с картинкой, пришедший разметкой (kind='string'), - тоже не файл.
test('S-001, S-003, S-004: из буфера отбираются файловые картинки, остальное - отдельно', () => {
  const items = [
    {kind: 'string', type: 'text/html'},
    {kind: 'string', type: 'image/png'},
    {kind: 'file', type: 'image/png', mark: 'первая'},
    {kind: 'file', type: 'text/plain'},
    {kind: 'file', type: 'image/svg+xml'},
    {kind: 'file', type: 'image/jpeg', mark: 'вторая'},
    {kind: 'file', type: 'image/svg+xml'},
  ];
  const {images, rejectedTypes} = clipboardImageItems(items);
  assert.deepEqual(images.map(item => item.mark), ['первая', 'вторая'], 'порядок буфера сохранён');
  assert.deepEqual(images.map(item => item.kind), ['file', 'file'], 'разметочный элемент картинкой не считается');
  // В отказ идут только картинки неподдерживаемого типа и без повторов:
  // текстовый файл картинкой не объявляется.
  assert.deepEqual(rejectedTypes, ['image/svg+xml']);
});

// S-013, S-014, S-023: файловый перенос опознаётся по типу 'Files'; внутренние
// переносы (письмо, событие календаря, строка настроек) его не содержат.
test('S-014, S-023: файловый перенос опознаётся по наличию типа Files', () => {
  [[['Files', 'text/plain'], true], [['Files'], true], [['application/x-truemail-messages'], false], [[], false], [undefined, false]]
    .forEach(([types, expected]) => assert.equal(isFileTransfer(types), expected, JSON.stringify(types)));
});

// S-008, S-028, S-035, S-041: разбор строки data: - тип и число двоичных
// байтов после раскодирования, иначе null.
test('S-008, S-028, S-035: разбор строки data: принимает только картинку в base64', () => {
  assert.deepEqual(parseDataUrl(dataUrl('image/png', 'hello')), {mimeType: 'image/png', byteLength: 5});
  // Регистр не важен ни у схемы, ни у метки кодирования.
  assert.deepEqual(parseDataUrl(`DATA:image/png;BASE64,${base64('hi')}`), {mimeType: 'image/png', byteLength: 2});
  const rejected = {
    'неподдерживаемый тип': dataUrl('image/svg+xml', '<svg/>'),
    // Тело разбираемое, метки base64 нет: без проверки метки эти данные ушли
    // бы в письмо как двоичные и размер посчитался бы по чужой длине.
    'нет метки base64': `data:image/png,${base64('hello')}`,
    'неразбираемое тело': 'data:image/png;base64,***not-base64***',
    // Схема не data:, но длина префикса та же: отрезание пяти символов без
    // проверки схемы приняло бы такую ссылку за картинку.
    'чужая схема той же длины': `blob:image/png;base64,${base64('hello')}`,
    'обычная ссылка': 'http://example.com/pic.png',
    'нет запятой': 'data:image/png;base64',
    'пусто': '',
    'ничего': null,
  };
  Object.entries(rejected).forEach(([reason, value]) => assert.equal(parseDataUrl(value), null, reason));
});

test('S-001, S-003: тег картинки собирается строкой data: с теми же типом и данными', () => {
  assert.equal(buildImageTag('image/png', 'QUJD'), '<img src="data:image/png;base64,QUJD">');
});

// S-012: тело из одних картинок без текста считается непустым.
test('S-012: тег img в теле находится независимо от регистра и атрибутов', () => {
  [['<p><br></p>', false], ['', false], ['<img src="data:image/png;base64,QUJD">', true],
    ["<IMG SRC='data:image/png;base64,QUJD'>", true], ['<div><img/></div>', true]]
    .forEach(([html, expected]) => assert.equal(htmlHasImageTag(html), expected, html));
});

// S-008, S-032: размер письма - вложения плюс байты картинок тела. Одинаковые
// картинки считаются один раз: ядро выносит их одной частью письма, и подсчёт
// с повторами давал бы отказ там, где письмо помещается.
test('S-008, S-032: размер письма складывает вложения и разные картинки тела', () => {
  const png = dataUrl('image/png', '12345');
  const other = dataUrl('image/png', '67');
  const cases = [
    ['пустое письмо', [], [], 0],
    ['ничего не передано', undefined, undefined, 0],
    ['только вложения', [100, 200], [], 300],
    ['вложения и картинка', [100, 200], [png], 305],
    ['чужая ссылка и неподдерживаемый тип не считаются', [100], [png, 'http://example.com/x.png', dataUrl('image/svg+xml', 'AAAA')], 105],
    ['повторы считаются один раз', [], [png, png, png], 5],
    ['разные картинки складываются', [], [png, other, png], 7],
  ];
  cases.forEach(([reason, sizes, sources, expected]) => assert.equal(totalMessageBytes(sizes, sources), expected, reason));
});

// --- Настоящий путь композера ---

const ACCOUNT = {id: 1, email: 'me@example.test'};

function composerApp() {
  const app = startApp({accounts: [ACCOUNT], bridge: {setSetting: () => null}});
  // Написание письма открыто: на закрытом композере брошенные файлы идут
  // другой веткой (открытие нового письма).
  app.document.getElementById('composeView').classList.add('active');
  return app;
}

const toastTexts = app => app.document.querySelectorAll('.app-toast-line').map(node => node.textContent);
const bodyShape = edit => edit.childNodes.map(node => (node.nodeType === 3 ? node.data : `<${node.tag}>`));
const imageSources = edit => edit.querySelectorAll('img').map(node => node.attributes.src);

// Курсор ставится внутрь текста тела: вставка мимо курсора роняет картинку в
// конец письма, и по одному наличию тега img это не видно.
function typeBody(app, text, caret) {
  const edit = app.document.getElementById('compEdit');
  edit.textContent = text;
  const range = app.document.createRange();
  range.setStart(edit.firstChild, caret);
  app.selection.addRange(range);
  return edit;
}

const clipboard = files => ({
  clipboardData: {items: files.map(file => ({kind: 'file', type: file.type, getAsFile: () => file}))},
});

test('S-001, S-003, S-005, S-010: две картинки встают по курсору друг за другом, стандартная вставка отменяется', async () => {
  const app = composerApp();
  await app.ready();
  const edit = typeBody(app, 'Привет мир', 7);
  const first = new FakeFile('one.png', 'image/png', Buffer.from('12345'));
  const second = new FakeFile('two.jpg', 'image/jpeg', Buffer.from('67'));
  const event = edit.dispatch('paste', clipboard([first, second]));
  // S-001, S-004: редактор не должен вставить тот же буфер ещё и сам.
  assert.equal(event.defaultPrevented, true, 'стандартная вставка отменена');
  await app.settle(40);
  assert.deepEqual(bodyShape(edit), ['Привет ', '<img>', '<img>', 'мир'], 'обе картинки встали по курсору, в порядке буфера');
  assert.deepEqual(imageSources(edit), [
    `data:image/png;base64,${base64('12345')}`,
    `data:image/jpeg;base64,${base64('67')}`,
  ]);
  // S-010: черновик сохраняется сам - программная вставка события ввода не
  // порождает, и без этого письмо с картинкой терялось до следующей клавиши.
  // Срок откладывания сохранения доигрывается часами окна, а не ожиданием
  // вживую: настоящая пауза удлиняет прогон и ничего не проверяет.
  await app.advanceTimers(700);
  const saved = app.calls.find(call => call.command === 'setSetting' && call.args[0] === 'composer_draft');
  assert.ok(saved, 'после вставки запускается сохранение черновика');
  assert.match(saved.args[1], /data:image\/png;base64/);
});

test('S-006, S-009: письмо сменилось, пока картинка читалась - в новое письмо она не попадает', async () => {
  const app = composerApp();
  await app.ready();
  const edit = typeBody(app, 'Старое письмо', 6);
  edit.dispatch('paste', clipboard([new FakeFile('one.png', 'image/png', Buffer.from('12345'))]));
  // Чтение файла асинхронное: письмо успевает смениться до вставки.
  app.run('resetComposer()');
  await app.settle(40);
  assert.deepEqual(imageSources(app.document.getElementById('compEdit')), [], 'картинка прежнего письма в новое не попала');
});

test('S-004: картинка неподдерживаемого типа не вставляется и названа в сообщении', async () => {
  const app = composerApp();
  await app.ready();
  const edit = typeBody(app, 'Текст', 5);
  const event = edit.dispatch('paste', clipboard([new FakeFile('pic.svg', 'image/svg+xml', Buffer.from('<svg/>'))]));
  await app.settle(20);
  assert.deepEqual(imageSources(edit), [], 'разметка svg в тело письма не попала');
  // Стандартную вставку отменяем и здесь: иначе редактор вставил бы её сам.
  assert.equal(event.defaultPrevented, true);
  assert.deepEqual(toastTexts(app), ['Не вставлено: неподдерживаемый тип картинки (image/svg+xml)']);
});

test('S-001: текстовый файл в буфере не объявляется неподдерживаемой картинкой', async () => {
  const app = composerApp();
  await app.ready();
  const edit = typeBody(app, 'Текст', 5);
  const event = edit.dispatch('paste', clipboard([new FakeFile('note.txt', 'text/plain', Buffer.from('привет'))]));
  await app.settle(20);
  assert.deepEqual(toastTexts(app), [], 'отказа по типу картинки быть не должно - картинки в буфере не было');
  // Вставка текста - дело редактора: отменять её нельзя, иначе текстовый
  // буфер перестал бы вставляться вовсе.
  assert.equal(event.defaultPrevented, false);
  assert.deepEqual(imageSources(edit), []);
});

test('S-003, S-007: картинка сверх предела письма пропускается, следующая рассматривается заново', async () => {
  const app = composerApp();
  await app.ready();
  // Вложение занимает всё письмо, кроме четырёх байтов. Значение важно только
  // размером, поэтому тело вложения не разворачиваем.
  app.set('composerAttachments', [{filename: 'big.bin', mime_type: 'application/octet-stream', data: {length: MAX_MESSAGE_BYTES - 4}}]);
  const edit = typeBody(app, 'Тело', 4);
  const big = new FakeFile('big.png', 'image/png', Buffer.from('12345'));
  const small = new FakeFile('small.png', 'image/png', Buffer.from('6789'));
  edit.dispatch('paste', clipboard([big, small]));
  await app.settle(40);
  // Граница включительная: пять байтов на четыре свободных не лезут, четыре - да.
  assert.deepEqual(imageSources(edit), [`data:image/png;base64,${base64('6789')}`], 'вставилась только та картинка, что помещается');
  assert.equal(toastTexts(app).length, 1);
  assert.match(toastTexts(app)[0], /^Не вставлено: картинка не помещается/);
});

test('S-018, S-019: из брошенных файлов прикладываются те, что помещаются, отказ называет остальные', async () => {
  const app = composerApp();
  await app.ready();
  const fitting = new FakeFile('note.txt', 'text/plain', Buffer.from('привет'));
  const huge = new FakeFile('huge.bin', 'application/octet-stream', MAX_MESSAGE_BYTES + 1);
  await app.sandbox.window.dropFilesToComposer([fitting, huge]);
  await app.settle(40);
  const attached = app.document.getElementById('compAtt').querySelectorAll('.att-name').map(node => node.textContent);
  assert.deepEqual(attached, ['note.txt'], 'приложенный файл остался, несмотря на отказ по следующему');
  assert.equal(toastTexts(app).filter(text => text.startsWith('Не добавлено: huge.bin')).length, 1);
  assert.ok(toastTexts(app).includes('Файлы добавлены к открытому письму'));
});
