// truemail UI module: folder-names.js
// Чистые функции без DOM и Tauri API: читаемая подпись папки для списка
// "Сквозные папки" (человекочитаемый путь вместо технического remote_path) и
// декодер modified UTF-7 (RFC 3501) для сегментов IMAP-пути. Подключается в
// index.html как обычный скрипт и отдаёт функции через один глобальный объект.
// См. specs/folder-names-readable.md.

// Декодирует один код "&...-" modified UTF-7 в UTF-16BE. Возвращает null при
// повреждённой последовательности (нет '-', плохой base64, нечётное число байт).
function decodeUtf16BeUnits(bytes) {
  const units = [];
  for (let i = 0; i < bytes.length; i += 2) units.push((bytes[i] << 8) | bytes[i + 1]);
  // Ручная сборка суррогатных пар вместо String.fromCharCode(...units):
  // одиночный (непарный) суррогат заменяется на U+FFFD, как в Rust decode_utf16,
  // а не остаётся в строке как есть.
  let out = '';
  for (let i = 0; i < units.length; i++) {
    const u = units[i];
    if (u >= 0xd800 && u <= 0xdbff) {
      const next = units[i + 1];
      if (next !== undefined && next >= 0xdc00 && next <= 0xdfff) {
        out += String.fromCharCode(u, next);
        i++;
      } else {
        out += '\uFFFD';
      }
    } else if (u >= 0xdc00 && u <= 0xdfff) {
      out += '\uFFFD';
    } else {
      out += String.fromCharCode(u);
    }
  }
  return out;
}

// Порт Rust decode_modified_utf7 (crates/core/src/backend/imap.rs) на JS: в
// интерфейсе такого декодера не было. Возвращает null на повреждённых данных -
// вызывающая сторона (decodeFolderSegment) в этом случае показывает сырую строку.
function decodeModifiedUtf7(value) {
  let out = '', rest = value;
  for (;;) {
    const start = rest.indexOf('&');
    if (start === -1) { out += rest; break; }
    out += rest.slice(0, start);
    rest = rest.slice(start + 1);
    const end = rest.indexOf('-');
    if (end === -1) return null;
    const encoded = rest.slice(0, end);
    if (encoded === '') {
      out += '&';
    } else {
      const standard = encoded.replace(/,/g, '/');
      const padded = standard + '='.repeat((4 - (standard.length % 4)) % 4);
      let bin;
      try { bin = atob(padded); } catch (e) { return null; }
      const bytes = Uint8Array.from(bin, ch => ch.charCodeAt(0));
      if (bytes.length % 2 !== 0) return null;
      out += decodeUtf16BeUnits(bytes);
    }
    rest = rest.slice(end + 1);
  }
  return out;
}

// Человекочитаемый сегмент пути IMAP. Повреждённая последовательность
// возвращается как есть (S-004) - это допустимое поведение, не ошибка.
function decodeFolderSegment(segment) {
  if (typeof segment !== 'string' || segment === '') return segment || '';
  const decoded = decodeModifiedUtf7(segment);
  return decoded === null ? segment : decoded;
}

// Собственная подпись папки: display_name после trim, иначе remote_path после
// trim, иначе пустая строка (S-004).
function folderOwnLabel(folder) {
  if (!folder) return '';
  const name = (folder.display_name || '').trim();
  if (name) return name;
  return (folder.remote_path || '').trim();
}

// Цепочка родителей от корня к папке по parent_id (Exchange). null, если
// родитель не найден в списке или обнаружен цикл - тогда используется только
// имя самой папки (S-002).
function parentChain(folder, foldersById) {
  const chain = [folder], visited = new Set([folder.id]);
  let current = folder;
  while (current.parent_id) {
    if (!foldersById.has(current.parent_id) || visited.has(current.parent_id)) return null;
    const parent = foldersById.get(current.parent_id);
    chain.unshift(parent);
    visited.add(parent.id);
    current = parent;
  }
  return chain;
}

// Подпись строки списка источников (S-001, S-002, S-004):
// 1) parent_id указывает на папку из списка (Exchange) - путь от корня по
//    цепочке человекочитаемых имён;
// 2) иначе remote_path с разделителями '/' или '|' (IMAP) - путь из
//    декодированных сегментов;
// 3) иначе display_name, а если и оно пусто - remote_path, а если и он пуст -
//    пустая строка.
function folderPathLabel(folder, foldersById) {
  if (!folder) return '';
  if (folder.parent_id && foldersById && foldersById.has(folder.parent_id)) {
    const chain = parentChain(folder, foldersById);
    if (chain) return chain.map(folderOwnLabel).join('/');
  }
  const remotePath = folder.remote_path || '';
  if (/[/|]/.test(remotePath)) {
    const segments = remotePath.split(/[/|]/).filter(part => part !== '');
    if (segments.length) return segments.map(decodeFolderSegment).join('/');
  }
  return folderOwnLabel(folder);
}

// Порядок ролей списка источников - тот же, что и в общем sortedFolders
// (mail.js), но общая функция не меняется (S-003): здесь своя копия таблицы.
const SOURCE_ROLE_ORDER = { inbox: 0, sent: 1, drafts: 2, archive: 3, spam: 4, trash: 5 };

// Сравнение двух записей {folder, label} для сортировки списка источников:
// сначала порядок ролей, затем подпись по алфавиту (S-003).
function compareFolderLabels(a, b) {
  const ar = SOURCE_ROLE_ORDER[a?.folder?.role] ?? 20;
  const br = SOURCE_ROLE_ORDER[b?.folder?.role] ?? 20;
  if (ar !== br) return ar - br;
  return String(a?.label || '').localeCompare(String(b?.label || ''), 'ru', { numeric: true, sensitivity: 'base' });
}

const folderNames = { folderPathLabel, compareFolderLabels, decodeFolderSegment };
if (typeof module !== 'undefined' && module.exports) module.exports = folderNames;
