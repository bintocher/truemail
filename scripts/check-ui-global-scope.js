// Проверка общей области имён у файлов интерфейса.
// Спецификация: specs/ui-global-scope-check.md.
// Запуск: node scripts/check-ui-global-scope.js
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const {findCollisions} = require('./ui-global-scope.js');

const root = path.join(__dirname, '..');
const indexPath = path.join(root, 'apps/desktop/ui/index.html');

// Порядок файлов берём из разметки, а не из перечня каталога: значение имеет
// именно порядок подключения, и он же определяет, какой файл упадёт вторым.
function scriptsFromHtml(html) {
  const found = [];
  const pattern = /<script\s+src="([^"?]+)/g;
  let match;
  while ((match = pattern.exec(html)) !== null) {
    if (match[1].endsWith('.js')) found.push(match[1]);
  }
  return found;
}

const html = fs.readFileSync(indexPath, 'utf8');
const files = [];
for (const src of scriptsFromHtml(html)) {
  const file = path.join(root, 'apps/desktop/ui', src);
  if (!fs.existsSync(file)) continue;
  files.push({path: `apps/desktop/ui/${src}`, text: fs.readFileSync(file, 'utf8')});
}

const collisions = findCollisions(files);
if (collisions.length) {
  console.error('Столкновение имён в общей области файлов интерфейса:');
  for (const item of collisions) {
    console.error(`  ${item.name}: объявлено в ${item.first} и снова в ${item.second}`);
  }
  console.error('');
  console.error('Файлы интерфейса подключаются обычными тегами script и делят одну область.');
  console.error('Повторное объявление через const, let или class роняет разбор второго файла,');
  console.error('и он не выполняется целиком. Переименуйте имя в одном из файлов.');
  process.exit(1);
}

console.log(`Проверено файлов интерфейса: ${files.length}`);
console.log('Столкновений имён в общей области нет.');
