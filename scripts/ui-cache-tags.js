// Чистая логика проверки меток версий файлов интерфейса.
// Спецификация: specs/ui-cache-tag-check.md.
// Без обращений к git и файловой системе: сюда приходят уже прочитанные тексты.
'use strict';

// Где записана метка версии файла интерфейса.
const HTML_HOSTS=['apps/desktop/ui/index.html','apps/desktop/ui/notify.html'];
const LOCALES_HOST='apps/desktop/ui/modules/i18n-onboarding.js';
const LOCALES_PREFIX='apps/desktop/ui/locales/';
const UI_PREFIX='apps/desktop/ui/';

// Метка подключения файла: ищем "<имя файла>?v=<метка>" в тексте подключающего
// файла. Возвращает null, если такого подключения нет.
function tagFor(hostText,fileName){
  if(!hostText)return null;
  const escaped=fileName.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');
  const match=hostText.match(new RegExp(`${escaped}\\?v=([^"'\`\\s>)]+)`));
  return match?match[1]:null;
}

// Метка файлов локализации: она записана строкой запроса в коде модуля.
function localesTag(hostText){
  if(!hostText)return null;
  const match=hostText.match(/locales\/\$\{locale\}\.json\?v=([^"'`\s>)]+)/);
  return match?match[1]:null;
}

// Подключающий файл для изменённого файла интерфейса: путь и функция чтения
// метки. null - файл по адресу с меткой не подключается (S-005).
function hostFor(filePath,readHost){
  if(filePath.startsWith(LOCALES_PREFIX)){
    return {host:LOCALES_HOST,read:localesTag};
  }
  const fileName=filePath.slice(filePath.lastIndexOf('/')+1);
  for(const host of HTML_HOSTS){
    // Ищем подключение в обоих состояниях: если метку убрали вместе с правкой,
    // в текущем состоянии её уже нет, а нарушение есть.
    if(tagFor(readHost(host,'head'),fileName)!==null||tagFor(readHost(host,'base'),fileName)!==null){
      return {host,read:text=>tagFor(text,fileName)};
    }
  }
  return null;
}

// Проверка набора изменений (S-001 - S-006). changes - массив
// {path, status}: status 'D' означает удалённый файл.
// readHost(path, side) отдаёт текст подключающего файла: side 'base' -
// состояние базовой ветки, 'head' - текущее.
// Возвращает массив нарушений {file, host, tag}.
function checkCacheTags(changes,readHost){
  const violations=[];
  for(const change of changes||[]){
    const filePath=change.path;
    if(!filePath.startsWith(UI_PREFIX))continue;          // S-004
    if(change.status==='D')continue;                       // S-006
    if(HTML_HOSTS.includes(filePath))continue;             // сам подключающий файл метки не несёт
    const host=hostFor(filePath,readHost);
    if(!host)continue;                                     // S-005
    const before=host.read(readHost(host.host,'base'));
    const after=host.read(readHost(host.host,'head'));
    if(before===null)continue;                             // подключения не было и раньше
    if(after===null){                                      // метка исчезла - файл перестал версионироваться
      violations.push({file:filePath,host:host.host,tag:'метка исчезла'});
      continue;
    }
    if(before===after){                                    // S-001, S-002
      violations.push({file:filePath,host:host.host,tag:after});
    }
  }
  return violations;
}

module.exports={checkCacheTags,tagFor,localesTag,HTML_HOSTS,LOCALES_HOST};
