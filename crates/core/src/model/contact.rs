//! Контакт (RFC 6350, vCard).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactEmail {
    pub email: String,
    /// home | work | other
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContactPhone {
    pub number: String,
    /// mobile | work | home | fax | other
    pub kind: Option<String>,
    pub extension: Option<String>,
}

impl ContactPhone {
    pub fn from_remote(value: &str, kind: Option<String>) -> Self {
        let value = value.trim().strip_prefix("tel:").unwrap_or(value.trim());
        let lower = value.to_lowercase();
        let markers = [";ext=", " ext. ", " доб. "];
        let split = markers
            .iter()
            .find_map(|marker| lower.rfind(marker).map(|index| (index, marker.len())));
        let (number, extension) = split.map_or_else(
            || (value.trim().to_owned(), None),
            |(index, marker_len)| {
                (
                    value[..index].trim().to_owned(),
                    Some(value[index + marker_len..].trim().to_owned()),
                )
            },
        );
        Self {
            number,
            kind,
            extension: extension.filter(|value| !value.is_empty()),
        }
    }

    pub fn remote_value(&self) -> String {
        match self
            .extension
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            Some(extension) => format!("{};ext={}", self.number.trim(), extension.trim()),
            None => self.number.trim().to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: Option<i64>,
    pub account_id: Option<i64>,
    pub uid: Option<String>,
    pub display_name: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub organization: Option<String>,
    pub emails: Vec<ContactEmail>,
    #[serde(default)]
    pub phones: Vec<ContactPhone>,
    pub is_favorite: bool,
    /// true, если контакт существует только в локальной БД - провайдер
    /// аккаунта не поддерживает запись контактов (см. auxiliary::write_contact),
    /// поэтому carddav/API-копии на сервере нет. Вычисляется из remote_url:
    /// пока его нет, контакт не отправлен ни на один сервер.
    #[serde(default)]
    pub is_local_only: bool,
}

pub fn clean_contact_name(value: &str) -> String {
    let mut value = value.trim();
    while value.len() >= 2 {
        let bytes = value.as_bytes();
        let quoted = (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[value.len() - 1] == b'"');
        if !quoted {
            break;
        }
        value = value[1..value.len() - 1].trim();
    }
    value.to_owned()
}

impl Contact {
    /// Инициалы для аватара.
    pub fn initials(&self) -> String {
        self.display_name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::clean_contact_name;

    #[test]
    fn removes_only_paired_outer_quotes_from_contact_names() {
        assert_eq!(clean_contact_name(" 'Alexey Makarov' "), "Alexey Makarov");
        assert_eq!(clean_contact_name("\"Elena Knyazkina\""), "Elena Knyazkina");
        assert_eq!(clean_contact_name("O'Connor"), "O'Connor");
        assert_eq!(clean_contact_name("'Unpaired"), "'Unpaired");
    }
}
