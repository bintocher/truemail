use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Keybinding {
    pub action: String,
    pub scope: String,
    pub combo: String,
}

/// Имена слотов - закрытый перечень из десяти точных строк. Разбор числа
/// принимал бы и "quick_step_01", и "quick_step_+1": такая строка заводила бы
/// в таблице горячих клавиш запись, которая занимает сочетание и не удаляется
/// ниоткуда (quick-steps.md, S-058 и S-059).
pub fn is_quick_step_key_action(action: &str) -> bool {
    (1..=10).any(|slot| action == format!("quick_step_{slot}"))
}

pub fn normalize_key_combo(combo: &str) -> Option<String> {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut meta = false;
    let mut key = None;
    for part in combo
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "alt" => alt = true,
            "shift" => shift = true,
            "meta" | "win" | "cmd" => meta = true,
            _ if key.is_none() => key = Some(part.to_owned()),
            _ => return None,
        }
    }
    let key = key?;
    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl".to_owned());
    }
    if alt {
        parts.push("Alt".to_owned());
    }
    if shift {
        parts.push("Shift".to_owned());
    }
    if meta {
        parts.push("Meta".to_owned());
    }
    parts.push(if key.len() == 1 {
        key.to_uppercase()
    } else if key.eq_ignore_ascii_case("delete") {
        "Del".into()
    } else {
        key
    });
    Some(parts.join("+"))
}
