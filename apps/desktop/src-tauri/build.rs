use std::path::Path;

fn main() {
    write_ui_version();
    tauri_build::build();
}

/// Зашить версию в интерфейс на сборке.
///
/// Подпись версии внизу боковой панели не должна зависеть ни от готовности
/// моста к ядру, ни от ответа команды: раньше она молча оставалась пустой,
/// если мост в момент загрузки окна был ещё не готов. Здесь сборка кладёт
/// номер версии и адрес выпуска прямо в файл интерфейса, а заодно ставит его
/// же меткой подключения - тогда встроенный браузер не подсунет старую копию.
/// См. specs/app-version-in-sidebar.md.
fn write_ui_version() {
    let version = env!("CARGO_PKG_VERSION");
    let ui = Path::new("../ui");
    let module = ui.join("modules/app-version.js");
    let contents = format!(
        "// Файл создаётся сборкой из версии пакета: править руками бессмысленно.\n\
         // См. specs/app-version-in-sidebar.md.\n\
         window.truemailVersion={{version:\"{version}\",releaseUrl:\"https://github.com/bintocher/truemail/releases/tag/v{version}\"}};\n"
    );
    // Пишем только при изменении: иначе каждая сборка трогала бы файл и сбивала
    // отслеживание изменений.
    if std::fs::read_to_string(&module).ok().as_deref() != Some(contents.as_str()) {
        let _ = std::fs::write(&module, &contents);
    }

    let index = ui.join("index.html");
    if let Ok(html) = std::fs::read_to_string(&index) {
        if let Some(updated) = with_version_tag(&html, version) {
            let _ = std::fs::write(&index, updated);
        }
    }
    println!("cargo:rerun-if-changed=../ui/index.html");
}

/// Заменить метку подключения файла версии на сам номер версии. Возвращает
/// None, если метка уже верная или строки подключения нет.
fn with_version_tag(html: &str, version: &str) -> Option<String> {
    const PREFIX: &str = "modules/app-version.js?v=";
    let start = html.find(PREFIX)? + PREFIX.len();
    let end = start + html[start..].find('"')?;
    if &html[start..end] == version {
        return None;
    }
    let mut updated = String::with_capacity(html.len());
    updated.push_str(&html[..start]);
    updated.push_str(version);
    updated.push_str(&html[end..]);
    Some(updated)
}
