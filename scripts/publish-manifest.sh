#!/usr/bin/env bash
# Собирает updater-манифест из артефактов релиза и публикует его в
# website/latest.json (GitVerse Pages = updater endpoint). Запускается отдельным
# job после параллельных сборок linux/macos/windows.
#
# Вызов: publish-manifest.sh <тег>. Ожидает TOKEN.
#
# Особенности GitVerse, вычлененные вручную:
# - attachments (browser_download_url) отдаются БЕЗ Authorization; с ним 401;
# - .json нельзя залить как ассет релиза (400), поэтому манифест кладём только
#   в website/latest.json через contents API.
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

release_id=$(curl -sS --fail-with-body -H "Authorization: Bearer $TOKEN" -H "$ACCEPT" \
  "$API/repos/$OWNER/$REPO/releases" | TAG="$TAG" python3 -c '
import json, os, sys
tag = os.environ["TAG"]
m = [r for r in json.load(sys.stdin) if r.get("tag_name") == tag]
print(m[0]["id"] if m else "")')
[ -n "$release_id" ] || { echo "Релиз $TAG не найден" >&2; exit 1; }

assets=$(curl -sS --fail-with-body -H "Authorization: Bearer $TOKEN" -H "$ACCEPT" \
  "$API/repos/$OWNER/$REPO/releases/$release_id/assets")

version="${TAG#v}"
manifest=$(ASSETS="$assets" VER="$version" TAG="$TAG" python3 <<'PY'
import json, os, subprocess, datetime
assets = json.loads(os.environ["ASSETS"])
by = {a["name"]: a for a in assets}

def dl(url):
    # Без Authorization - иначе GitVerse отдаёт 401.
    return subprocess.run(["curl", "-sL", url], capture_output=True, text=True).stdout.strip()

specs = [
    ("windows-x86_64", ".exe.zip", lambda n: n[:-4] + ".sig.txt"),   # setup.exe.zip -> setup.exe.sig.txt
    ("linux-x86_64",   ".AppImage", lambda n: n + ".sig.txt"),
    ("darwin-aarch64", ".app.tar.gz", lambda n: n + ".sig.txt"),
]
platforms = {}
for key, suf, signame in specs:
    pkg = next((a for a in assets if a["name"].endswith(suf)), None)
    if not pkg:
        continue
    sig = by.get(signame(pkg["name"]))
    if not sig:
        continue
    platforms[key] = {
        "signature": dl(sig["browser_download_url"]),
        "url": pkg["browser_download_url"],
    }
if not platforms:
    raise SystemExit("нет ни одной платформы для манифеста")
m = {
    "version": os.environ["VER"],
    "notes": "truemail update " + os.environ["TAG"],
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "platforms": platforms,
}
print(json.dumps(m, ensure_ascii=False, indent=2))
PY
)

printf '%s\n' "$manifest" > latest.json
echo "Собран latest.json ($(printf '%s' "$manifest" | python3 -c 'import json,sys;print(", ".join(json.load(sys.stdin)["platforms"]))'))"

# Публикуем только в website/latest.json (updater endpoint). .json нельзя залить
# ассетом релиза. Обновление или создание файла (sha только если файл есть).
sha=$(curl -sS -H "Authorization: Bearer $TOKEN" -H "$ACCEPT" \
  "$API/repos/$OWNER/$REPO/contents/website/latest.json?ref=master" \
  | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("sha",""))
except Exception: print("")' 2>/dev/null || true)
encoded=$(base64 -w0 latest.json 2>/dev/null || base64 latest.json | tr -d '\n')
body=$(SHA="$sha" ENC="$encoded" TAG="$TAG" python3 -c '
import json, os
b = {"branch":"master","content":os.environ["ENC"],
     "message":"chore: publish updater manifest for %s [skip ci]" % os.environ["TAG"],"signoff":False}
if os.environ["SHA"]: b["sha"] = os.environ["SHA"]
print(json.dumps(b))')
printf '%s' "$body" | curl -sS --fail-with-body -H "Authorization: Bearer $TOKEN" -H "$ACCEPT" \
  -H 'Content-Type: application/json' -X PUT -d @- \
  "$API/repos/$OWNER/$REPO/contents/website/latest.json" >/dev/null
echo "website/latest.json опубликован"
