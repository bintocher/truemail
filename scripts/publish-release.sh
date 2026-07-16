#!/usr/bin/env bash
# Публикация релиза truemail в GitVerse.
#
# Вызов: publish-release.sh <тег> <каталог с файлами>
# Ожидает переменную TOKEN - токен GitVerse с правом на запись в репозиторий.
#
# Релиз создаётся только если тега ещё нет: master собирается на каждый push,
# а выпускать релиз нужно, когда версия в Cargo.toml выросла.
set -euo pipefail

TAG="${1:?не указан тег релиза}"
DIR="${2:?не указан каталог с файлами}"
OWNER="chernov"
REPO="truemail"
API="https://api.gitverse.ru"
# Без этого заголовка API отвечает 400 с пустым телом. version=1 - рабочая версия.
ACCEPT="Accept: application/vnd.gitverse.object+json;version=1"

if [ -z "${TOKEN:-}" ]; then
  echo "TOKEN не задан: добавьте секрет TRUEMAIL_GITVERSE_TOKEN в настройках репозитория" >&2
  exit 1
fi

api() {
  curl -sS --fail-with-body -H "Authorization: Bearer $TOKEN" -H "$ACCEPT" "$@"
}

existing=$(api "$API/repos/$OWNER/$REPO/releases" | tr ',' '\n' | grep -c "\"$TAG\"" || true)
if [ "$existing" != "0" ]; then
  echo "Релиз $TAG уже опубликован - пропускаю. Подняли версию в Cargo.toml?"
  exit 0
fi

echo "Создаю релиз $TAG"
release=$(api -X POST "$API/repos/$OWNER/$REPO/releases" \
  -H 'Content-Type: application/json' \
  -d "{\"tag_name\":\"$TAG\",\"name\":\"truemail $TAG\",\"body\":\"Сборки для Linux: deb, rpm, AppImage.\"}")

release_id=$(printf '%s' "$release" | tr ',' '\n' | grep -m1 '"id"' | tr -dc '0-9')
if [ -z "$release_id" ]; then
  echo "Не удалось получить id релиза. Ответ: $release" >&2
  exit 1
fi

shopt -s nullglob
files=("$DIR"/*)
if [ ${#files[@]} -eq 0 ]; then
  echo "В каталоге $DIR нет файлов для загрузки" >&2
  exit 1
fi

for file in "${files[@]}"; do
  echo "Загружаю $(basename "$file")"
  api -X POST "$API/repos/$OWNER/$REPO/releases/$release_id/assets" \
    -F "attachment=@$file" >/dev/null
done

echo "Релиз $TAG опубликован: ${#files[@]} файлов"
