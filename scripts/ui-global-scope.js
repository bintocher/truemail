// Чистая логика проверки общей области имён у файлов интерфейса.
// Спецификация: specs/ui-global-scope-check.md.
// Без обращений к файловой системе: сюда приходят уже прочитанные тексты.
'use strict';

// Объявления верхнего уровня, которые попадают в общую область. Модули
// интерфейса подключаются обычными тегами script без сборщика, поэтому все они
// делят одну область: второе объявление того же имени через const, let или
// class роняет разбор целого файла, и он не выполняется совсем.
// После const, let, var и class обязателен пробел, иначе слово вроде constant
// разобралось бы как объявление. После function допустима ещё и звёздочка
// генератора, но не сразу буква: имя functionName объявлением не является.
const DECLARATION = /^(?:(?:const|let|var|class)\s+|function(?:\s+|\s*\*\s*))([A-Za-z_$][\w$]*)/;
// Строка объявления должна начинаться с первой колонки: всё, что с отступом,
// лежит внутри функции или блока и в общую область не попадает.
const TOP_LEVEL = /^[A-Za-z]/;

// Однострочные и многострочные примечания убираем целиком, строковые значения
// заменяем пустыми: иначе слово const внутри текста или примера сойдёт за
// объявление.
function stripNoise(text) {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/(^|[^:])\/\/[^\n]*/g, '$1')
    .replace(/`(?:\\.|[^`\\])*`/g, '``')
    .replace(/'(?:\\.|[^'\\\n])*'/g, "''")
    .replace(/"(?:\\.|[^"\\\n])*"/g, '""');
}

// Имена верхнего уровня одного файла. Повтор имени внутри файла нас не
// занимает: его поймает разбор самого файла.
function topLevelNames(text) {
  const names = new Set();
  if (!text) return names;
  for (const line of stripNoise(text).split('\n')) {
    if (!TOP_LEVEL.test(line)) continue;
    const found = DECLARATION.exec(line);
    if (found) names.add(found[1]);
  }
  return names;
}

// Столкновения имён между файлами. Порядок файлов - порядок подключения:
// сообщение называет тот файл, который перестанет выполняться.
function findCollisions(files) {
  const owners = new Map();
  const collisions = [];
  for (const file of files) {
    for (const name of topLevelNames(file.text)) {
      const first = owners.get(name);
      if (first) collisions.push({name, first, second: file.path});
      else owners.set(name, file.path);
    }
  }
  return collisions;
}

module.exports = {topLevelNames, findCollisions, stripNoise};
