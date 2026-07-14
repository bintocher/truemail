# Бренд truemail

Основной знак — белая буква `T`, образованная клапаном конверта. Конверт
располагается по центру прозрачного квадратного холста. Все платформенные
иконки генерируются из `truemail-app-icon.svg`.

## Файлы

- `truemail-mark.svg` — самостоятельный знак на квадратном холсте;
- `truemail-app-icon.svg` — мастер для платформенных иконок;
- `truemail-logo.svg` — полный горизонтальный логотип для светлого фона;
- `truemail-logo-dark.svg` — полный логотип для тёмного фона;
- `truemail-wordmark.svg` — текстовая часть без знака.
- `truemail-app-icon.png` — квадратный PNG-мастер 1024×1024;
- `truemail-logo.png` — растровая версия принятого полного логотипа;
- `truemail-logo-reference.png` — исходный лист утверждённой концепции;
- `truemail-app-icon-reference.png` — исходная растровая концепция app icon.

## Цвета

- основной индиго: `#5B63D3`;
- индиго для тёмной темы: `#7C84F0`;
- основной текст: `#17181C`;
- белый: `#FFFFFF`.

## Перегенерация иконок

Из корня репозитория:

```sh
cargo tauri icon assets/brand/truemail-app-icon.svg -o apps/desktop/src-tauri/icons
```

Не редактируйте платформенные PNG, ICO и ICNS вручную: источником истины
является `truemail-app-icon.svg`.
