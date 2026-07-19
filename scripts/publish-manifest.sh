#!/usr/bin/env bash
# Собирает updater-манифест latest.json из уже загруженных ассетов релиза и
# публикует его. Запускается ОТДЕЛЬНЫМ job после параллельных сборок linux/macos/
# windows - поэтому в релизе к этому моменту есть артефакты всех платформ.
#
# Вызов: publish-manifest.sh <тег>
# Ожидает TOKEN - токен GitVerse с правом на запись.
set -euo pipefail

TAG="${1:?не указан тег релиза}"
OWNER="chernov"
REPO="truemail"
API="https://api.gitverse.ru"
ACCEPT="Accept: application/vnd.gitverse.object+json;version=1"

if [ -z "${TOKEN:-}" ]; then
  echo "TOKEN не задан" >&2
  exit 1
fi

api() { curl -sS --fail-with-body -H "Authorization: Bearer $TOKEN" -H "$ACCEPT" "$@"; }

release_id=$(api "$API/repos/$OWNER/$REPO/releases" | TAG="$TAG" python3 -c '
import json, os, sys
tag = os.environ["TAG"]
m = [r for r in json.load(sys.stdin) if r.get("tag_name") == tag]
print(m[0]["id"] if m else "")')
if [ -z "$release_id" ]; then
  echo "Релиз $TAG не найден" >&2
  exit 1
fi

assets_json=$(api "$API/repos/$OWNER/$REPO/releases/$release_id/assets")

# Скачать текст ассета-подписи по имени (.sig.txt лежит как обычный файл).
sig_text() {
  local url="$1"
  curl -sSL -H "Authorization: Bearer $TOKEN" -H "$ACCEPT" "$url"
}

version="${TAG#v}"

# Собираем platforms{} из троек: ключ платформы, шаблон пакета, шаблон подписи.
# Подпись windows - от .exe внутри .zip, поэтому .exe.sig.txt (без .zip).
manifest=$(ASSETS="$assets_json" VER="$version" TAG="$TAG" python3 - "$TOKEN" <<'PY'
import json, os, sys, urllib.request

token = sys.argv[1]
assets = json.loads(os.environ["ASSETS"])
by_name = {a["name"]: a for a in assets}

def find(suffix, exclude=None):
    for a in assets:
        n = a["name"]
        if n.endswith(suffix) and (exclude is None or not n.endswith(exclude)):
            return a
    return None

def sig_text(url):
    req = urllib.request.Request(url, headers={
        "Authorization": "Bearer " + token,
        "Accept": "application/vnd.gitverse.object+json;version=1",
    })
    with urllib.request.urlopen(req) as r:
        return r.read().decode("utf-8").strip()

platforms = {}
# (ключ, пакет-suffix, exclude, база-подписи)
specs = [
    ("windows-x86_64", ".exe.zip", None, lambda n: n[:-4]),          # setup.exe.zip -> setup.exe(.sig.txt)
    ("linux-x86_64",   ".AppImage", None, lambda n: n),
    ("darwin-aarch64", ".app.tar.gz", None, lambda n: n),
]
for key, suf, exc, sigbase in specs:
    pkg = find(suf, exc)
    if not pkg:
        continue
    signame = sigbase(pkg["name"]) + ".sig.txt"
    sig = by_name.get(signame)
    if not sig:
        sys.stderr.write("нет подписи %s для %s\n" % (signame, pkg["name"]))
        continue
    platforms[key] = {
        "signature": sig_text(sig["browser_download_url"]),
        "url": pkg["browser_download_url"],
    }

if not platforms:
    sys.stderr.write("не найдено ни одной платформы для манифеста\n")
    sys.exit(1)

manifest = {
    "version": os.environ["VER"],
    "notes": "truemail update " + os.environ["TAG"],
    "pub_date": "1970-01-01T00:00:00Z",
    "platforms": platforms,
}
print(json.dumps(manifest, ensure_ascii=False, indent=2))
PY
)

# Проставить реальную дату публикации (Python в CI без сети времени берёт из системы).
manifest=$(printf '%s' "$manifest" | python3 -c '
import json, sys, datetime
d = json.load(sys.stdin)
d["pub_date"] = datetime.datetime.utcnow().strftime("%Y-%m-%dT%H:%M:%SZ")
print(json.dumps(d, ensure_ascii=False, indent=2))')

printf '%s\n' "$manifest" > latest.json
echo "Собран latest.json:"; cat latest.json

# Залить latest.json как ассет (перезаписать, если уже есть).
existing_id=$(printf '%s' "$assets_json" | python3 -c '
import json, sys
m = [a["id"] for a in json.load(sys.stdin) if a["name"] == "latest.json"]
print(m[0] if m else "")')
if [ -n "$existing_id" ]; then
  api -X DELETE "$API/repos/$OWNER/$REPO/releases/$release_id/assets/$existing_id" >/dev/null || true
fi
api -X POST "$API/repos/$OWNER/$REPO/releases/$release_id/assets" \
  -F "attachment=@latest.json" -F "name=latest.json" >/dev/null
echo "latest.json загружен в релиз"

# Обновить website/latest.json для GitVerse Pages (updater endpoint). Не фатально.
if content=$(api "$API/repos/$OWNER/$REPO/contents/website/latest.json?ref=master" 2>/dev/null); then
  sha=$(printf '%s' "$content" | python3 -c 'import json,sys;print(json.load(sys.stdin).get("sha",""))')
else
  sha=""
fi
encoded=$(base64 -w0 latest.json 2>/dev/null || base64 latest.json | tr -d '\n')
body=$(python3 -c '
import json, sys
sha = sys.argv[1]
b = {"branch":"master","content":sys.argv[2],"message":"chore: publish updater manifest for %s [skip ci]" % sys.argv[3],"signoff":False}
if sha: b["sha"] = sha
print(json.dumps(b))' "$sha" "$encoded" "$TAG")
printf '%s' "$body" | api -X PUT -H 'Content-Type: application/json' -d @- \
  "$API/repos/$OWNER/$REPO/contents/website/latest.json" >/dev/null 2>&1 \
  && echo "website/latest.json обновлён" \
  || echo "website/latest.json не обновлён (не фатально)"
