// Проверки чистой логики apps/desktop/ui/modules/composer-body.js.
// Запуск: node --test apps/desktop/tests/js/composer-body.test.js (Node 20+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {
  MAX_MESSAGE_BYTES, isSupportedImageType, isFileTransfer, clipboardImageItems,
  parseDataUrl, buildImageTag, htmlHasImageTag, totalMessageBytes, fitsMessageLimit,
} = require('../../ui/modules/composer-body.js');

// isSupportedImageType / clipboardImageItems (S-001, S-003, S-004).
test('isSupportedImageType: перечень поддерживаемых типов картинок', () => {
  assert.equal(isSupportedImageType('image/png'), true);
  assert.equal(isSupportedImageType('image/jpeg'), true);
  assert.equal(isSupportedImageType('image/gif'), true);
  assert.equal(isSupportedImageType('image/webp'), true);
  assert.equal(isSupportedImageType('image/bmp'), true);
  assert.equal(isSupportedImageType('image/svg+xml'), false);
  assert.equal(isSupportedImageType('IMAGE/PNG'), true);
});
test('S-001, S-003: clipboardImageItems отбирает файловые картинки поддерживаемых типов в порядке буфера', () => {
  const items = [
    {kind: 'string', type: 'text/plain'},
    {kind: 'file', type: 'image/png'},
    {kind: 'string', type: 'text/html'},
    {kind: 'file', type: 'image/jpeg'},
  ];
  const {images, rejectedTypes} = clipboardImageItems(items);
  assert.deepEqual(images.map(i => i.type), ['image/png', 'image/jpeg']);
  assert.deepEqual(rejectedTypes, []);
});
test('S-004: clipboardImageItems отклоняет image/svg+xml и сообщает тип', () => {
  const items = [{kind: 'file', type: 'image/svg+xml'}, {kind: 'file', type: 'image/png'}];
  const {images, rejectedTypes} = clipboardImageItems(items);
  assert.deepEqual(images.map(i => i.type), ['image/png']);
  assert.deepEqual(rejectedTypes, ['image/svg+xml']);
});
test('clipboardImageItems: пустой и без файловых элементов буфер', () => {
  assert.deepEqual(clipboardImageItems([]), {images: [], rejectedTypes: []});
  assert.deepEqual(clipboardImageItems([{kind: 'string', type: 'text/plain'}]), {images: [], rejectedTypes: []});
});

// S-014, S-023: распознавание файлового переноса по перечню типов.
test('S-014: isFileTransfer - перенос с типом Files распознаётся как файловый', () => {
  assert.equal(isFileTransfer(['Files', 'text/plain']), true);
  assert.equal(isFileTransfer(['Files']), true);
});
test('S-014: isFileTransfer - внутренний перенос без Files файловым не считается', () => {
  assert.equal(isFileTransfer(['application/x-truemail-messages']), false);
  assert.equal(isFileTransfer([]), false);
  assert.equal(isFileTransfer(undefined), false);
});

// parseDataUrl (S-008, S-028, S-035, S-041).
test('parseDataUrl: валидная строка data: с поддерживаемым типом', () => {
  const base64 = Buffer.from('hello').toString('base64');
  const result = parseDataUrl(`data:image/png;base64,${base64}`);
  assert.deepEqual(result, {mimeType: 'image/png', byteLength: 5});
});
test('parseDataUrl: регистр не важен для data: и base64', () => {
  const base64 = Buffer.from('hi').toString('base64');
  const result = parseDataUrl(`DATA:image/png;BASE64,${base64}`);
  assert.deepEqual(result, {mimeType: 'image/png', byteLength: 2});
});
test('parseDataUrl: неподдерживаемый тип (svg) возвращает null', () => {
  const base64 = Buffer.from('<svg/>').toString('base64');
  assert.equal(parseDataUrl(`data:image/svg+xml;base64,${base64}`), null);
});
test('parseDataUrl: данные не в base64 (нет метки base64) возвращают null', () => {
  assert.equal(parseDataUrl('data:image/png,notbase64'), null);
});
test('parseDataUrl: неразбираемые данные base64 возвращают null', () => {
  assert.equal(parseDataUrl('data:image/png;base64,***not-base64***'), null);
});
test('parseDataUrl: строка не data: возвращает null', () => {
  assert.equal(parseDataUrl('http://example.com/pic.png'), null);
  assert.equal(parseDataUrl(''), null);
  assert.equal(parseDataUrl(null), null);
});
test('parseDataUrl: без запятой (нет данных) возвращает null', () => {
  assert.equal(parseDataUrl('data:image/png;base64'), null);
});

// buildImageTag (S-001, S-003).
test('S-001: buildImageTag собирает тег img со строкой data:', () => {
  assert.equal(buildImageTag('image/png', 'QUJD'), '<img src="data:image/png;base64,QUJD">');
});

// htmlHasImageTag (S-012).
test('S-012: htmlHasImageTag находит тег img независимо от атрибутов', () => {
  assert.equal(htmlHasImageTag('<p><br></p>'), false);
  assert.equal(htmlHasImageTag(''), false);
  assert.equal(htmlHasImageTag('<img src="data:image/png;base64,QUJD">'), true);
  assert.equal(htmlHasImageTag('<IMG SRC=\'data:image/png;base64,QUJD\'>'), true);
  assert.equal(htmlHasImageTag('<div><img/></div>'), true);
});

// totalMessageBytes (S-008).
test('S-008: totalMessageBytes считает байты вложений и разбираемых картинок в теле', () => {
  const base64 = Buffer.from('12345').toString('base64');
  const attachmentSizes = [100, 200];
  const imgSources = [`data:image/png;base64,${base64}`, 'http://example.com/x.png', 'data:image/svg+xml;base64,AAAA'];
  assert.equal(totalMessageBytes(attachmentSizes, imgSources), 100 + 200 + 5);
});
test('S-008: totalMessageBytes пересчитывается заново после удаления картинки из тела', () => {
  const base64 = Buffer.from('12345').toString('base64');
  const withImage = totalMessageBytes([100], [`data:image/png;base64,${base64}`]);
  const withoutImage = totalMessageBytes([100], []);
  assert.equal(withImage, 105);
  assert.equal(withoutImage, 100);
});
test('S-032: одна и та же картинка в теле считается один раз', () => {
  // Ядро выносит совпадающие строки data: одной частью письма, поэтому
  // повторы не должны раздувать подсчёт и давать ложный отказ по пределу.
  const src = `data:image/png;base64,${Buffer.from('12345').toString('base64')}`;
  assert.equal(totalMessageBytes([], [src, src, src]), 5);
  const other = `data:image/png;base64,${Buffer.from('67').toString('base64')}`;
  assert.equal(totalMessageBytes([], [src, other, src]), 7);
});
test('totalMessageBytes: пустые списки дают ноль', () => {
  assert.equal(totalMessageBytes([], []), 0);
  assert.equal(totalMessageBytes(undefined, undefined), 0);
});

// fitsMessageLimit (S-003, S-007, S-009, S-018, S-038).
test('S-007: fitsMessageLimit - ровно предел допустим, предел плюс один байт - нет', () => {
  assert.equal(fitsMessageLimit(0, MAX_MESSAGE_BYTES), true);
  assert.equal(fitsMessageLimit(0, MAX_MESSAGE_BYTES + 1), false);
  assert.equal(fitsMessageLimit(MAX_MESSAGE_BYTES - 10, 10), true);
  assert.equal(fitsMessageLimit(MAX_MESSAGE_BYTES - 10, 11), false);
});
test('S-003: картинка, не влезающая в предел, пропускается, следующая рассматривается заново', () => {
  // Тот же порядок, каким идёт вставка нескольких картинок одной вставкой:
  // предел проверяется перед каждой картинкой по уже накопленной сумме.
  const sizes = [10, MAX_MESSAGE_BYTES, 5];
  let total = 0;
  const inserted = [];
  sizes.forEach((size, index) => {
    if (!fitsMessageLimit(total, size)) return;
    total += size;
    inserted.push(index);
  });
  assert.deepEqual(inserted, [0, 2]);
  assert.equal(total, 15);
});
test('S-018: брошенные файлы - часть влезает, часть нет, уже приложенные остаются', () => {
  const sizes = [100, MAX_MESSAGE_BYTES];
  let total = 0;
  const attached = [];
  sizes.forEach((size, index) => {
    if (!fitsMessageLimit(total, size)) return;
    total += size;
    attached.push(index);
  });
  assert.deepEqual(attached, [0]);
});
