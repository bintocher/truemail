#!/usr/bin/env bash
# Публикация и дополнение релиза truemail в GitVerse.
#
# Вызов: publish-release.sh <тег> <каталог с файлами>
# Ожидает переменную TOKEN - токен GitVerse с правом на запись в репозиторий.
#
# Скрипт идемпотентен: находит релиз по тегу или создаёт его, затем догружает
# только недостающие файлы. Так каждый платформенный job (linux, macos) вызывает
# один и тот же скрипт и добавляет свои артефакты в общий релиз. Релиз создаётся
# только под новый тег - master собирается на каждый push, а выпуск нужен, когда
# версия в Cargo.toml выросла.
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

# Найти релиз по тегу. Пустой вывод - релиза ещё нет.
release_id=$(api "$API/repos/$OWNER/$REPO/releases" | TAG="$TAG" python3 -c '
import json, os, sys
tag = os.environ["TAG"]
match = [r for r in json.load(sys.stdin) if r.get("tag_name") == tag]
print(match[0]["id"] if match else "")')

if [ -z "$release_id" ]; then
  echo "Создаю релиз $TAG"
  release=$(api -X POST "$API/repos/$OWNER/$REPO/releases" \
    -H 'Content-Type: application/json' \
    -d "{\"tag_name\":\"$TAG\",\"name\":\"truemail $TAG\",\"body\":\"Сборки truemail для Linux, Windows и macOS.\",\"is_authorized_only\":false}")
  release_id=$(printf '%s' "$release" | python3 -c '
import json, sys
print(json.load(sys.stdin).get("id", ""))')
  if [ -z "$release_id" ]; then
    echo "Не удалось получить id релиза. Ответ: $release" >&2
    exit 1
  fi
else
  echo "Релиз $TAG уже создан (id $release_id) - догружаю недостающие файлы"
fi

# Имена уже загруженных ассетов - чтобы не заливать один файл дважды при
# повторном или последовательном запуске платформенных job.
existing=$(api "$API/repos/$OWNER/$REPO/releases/$release_id/assets" | python3 -c '
import json, sys
print("\n".join(a.get("name", "") for a in json.load(sys.stdin)))')

# deb и rpm GitVerse не принимает как файлы релиза (400), поэтому пакуем их
# в tar.gz. AppImage, dmg, app.tar.gz и подписи заливаются как есть.
shopt -s nullglob
for pkg in "$DIR"/*.deb "$DIR"/*.rpm; do
  (cd "$DIR" && tar czf "$(basename "$pkg").tar.gz" "$(basename "$pkg")" && rm -f "$(basename "$pkg")")
done

files=("$DIR"/*)
if [ ${#files[@]} -eq 0 ]; then
  echo "В каталоге $DIR нет файлов для загрузки" >&2
  exit 1
fi

uploaded=0
for file in "${files[@]}"; do
  name=$(basename "$file")
  if printf '%s\n' "$existing" | grep -qxF "$name"; then
    echo "Файл $name уже в релизе - пропускаю"
    continue
  fi
  echo "Загружаю $name"
  # name обязателен отдельным полем формы: без него 422, в query-параметре - 400.
  api -X POST "$API/repos/$OWNER/$REPO/releases/$release_id/assets" \
    -F "attachment=@$file" -F "name=$name" >/dev/null
  uploaded=$((uploaded + 1))
done

echo "Релиз $TAG: загружено новых файлов - $uploaded"
