//! Запись файлов, которую нельзя оборвать на середине.
//!
//! `fs::write` обрезает файл и потом заполняет: процесс, убитый в этот момент,
//! оставляет огрызок. Для настроек это неприятно, для правил и коллекций —
//! потеря работы, потому что читатель видит либо ошибку разбора, либо пустоту.
//!
//! Пишем во временный файл рядом и переименовываем: `rename` в пределах одной
//! файловой системы атомарен, поэтому читатель видит либо прежнюю версию, либо
//! новую целиком. Предыдущее содержимое остаётся рядом как `.bak` — на случай,
//! когда испортило не нас, а нам.

use std::fs;
use std::io;
use std::path::Path;

/// Записать `contents` в `path` целиком или не записать вовсе.
pub fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }

    // Прежнее значение — рядом, до того как мы его тронули.
    if let Ok(previous) = fs::read(path) {
        if previous != contents.as_ref() {
            let _ = fs::write(with_suffix(path, "bak"), previous);
        }
    }

    let tmp = with_suffix(path, "tmp");
    fs::write(&tmp, contents)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Не оставляем мусор рядом с настоящим файлом.
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// `rules.json` → `rules.json.bak`. Суффикс, а не замена расширения: имя файла
/// остаётся узнаваемым, и `.bak` от `.json` отличается на глаз.
fn with_suffix(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("trawl-atomic-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_the_contents_and_leaves_no_temporary_behind() {
        let dir = scratch("basic");
        let path = dir.join("rules.json");
        write_atomic(&path, "[]").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "[]");
        assert!(!dir.join("rules.json.tmp").exists());
    }

    #[test]
    fn keeps_the_previous_contents_beside_the_file() {
        let dir = scratch("backup");
        let path = dir.join("rules.json");
        write_atomic(&path, "[1]").unwrap();
        write_atomic(&path, "[1,2]").unwrap();

        assert_eq!(fs::read_to_string(dir.join("rules.json.bak")).unwrap(), "[1]");
    }

    #[test]
    fn writing_the_same_contents_twice_keeps_the_older_backup() {
        let dir = scratch("same");
        let path = dir.join("rules.json");
        write_atomic(&path, "one").unwrap();
        write_atomic(&path, "two").unwrap();
        write_atomic(&path, "two").unwrap();

        // Сохранение без изменений не должно вытеснять единственную хорошую копию.
        assert_eq!(fs::read_to_string(dir.join("rules.json.bak")).unwrap(), "one");
    }

    #[test]
    fn creates_the_directory_when_it_is_missing() {
        let dir = scratch("mkdir").join("nested");
        write_atomic(&dir.join("x.json"), "{}").unwrap();

        assert_eq!(fs::read_to_string(dir.join("x.json")).unwrap(), "{}");
    }
}
