// truemail UI module: message-labels.js
// Чистые функции без DOM и Tauri API: отбор показываемых меток письма, цвет
// метки по имени, значение полосы у края строки и модель точек в первой
// строке. Подключается в index.html перед mail.js как обычный скрипт и отдаёт
// функции через один глобальный объект.
// См. specs/message-labels-visible.md.

// Нейтральный цвет: им рисуется метка, которой нет в перечне, и сегмент
// "остальные" у письма с четырьмя и более метками (S-004, S-009).
const LABEL_NEUTRAL = '#8a8f98';
// Больше трёх цветов в 57 пикселях строки списка неразличимы, поэтому и полоса,
// и точки показывают три метки, а остаток сводят в один элемент.
const LABEL_LIMIT = 3;

// Цвет метки по имени. Имя - ключ: labels.name объявлено UNIQUE в базе, двух
// меток с одним именем быть не может.
function labelColor(name, tags) {
  const tag = (tags || []).find(item => item && item.name === name);
  const color = tag && typeof tag.color === 'string' ? tag.color.trim() : '';
  return color || LABEL_NEUTRAL;
}

// Метки, которые показываются в строке списка. В списке самой метки её же цвет
// ничего не говорит - он у всех писем списка, поэтому она скрывается (S-010).
function shownLabels(labels, openTag) {
  const names = (labels || []).filter(name => typeof name === 'string' && name.length);
  if (openTag == null) return names;
  return names.filter(name => name !== openTag);
}

// Значение фона полосы у левого края строки: одна метка - сплошной цвет,
// несколько - равные сегменты сверху вниз в порядке меток письма (S-001, S-003).
function stripeValue(names, tags) {
  const shown = names || [];
  if (!shown.length) return '';
  const colors = shown.slice(0, LABEL_LIMIT).map(name => labelColor(name, tags));
  if (shown.length > LABEL_LIMIT) colors.push(LABEL_NEUTRAL);
  if (colors.length === 1) return colors[0];
  const step = 100 / colors.length;
  const stops = colors.map((color, index) => {
    const from = (index * step).toFixed(3);
    const to = ((index + 1) * step).toFixed(3);
    return `${color} ${from}% ${to}%`;
  });
  return `linear-gradient(to bottom,${stops.join(',')})`;
}

// Модель точек в первой строке: цвета первых трёх меток и число остальных
// (S-005, S-006). Ноль в more означает, что счётчика нет.
function dotsModel(names, tags) {
  const shown = names || [];
  return {
    dots: shown.slice(0, LABEL_LIMIT).map(name => labelColor(name, tags)),
    more: Math.max(0, shown.length - LABEL_LIMIT),
  };
}

const messageLabels = { labelColor, shownLabels, stripeValue, dotsModel, LABEL_NEUTRAL, LABEL_LIMIT };
if (typeof module !== 'undefined' && module.exports) module.exports = messageLabels;
