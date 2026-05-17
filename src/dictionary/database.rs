use alfred::core::AlfredUtils;
use rusqlite::{Connection, OpenFlags};

use super::entry::StardictEntry;

const TABLE: &str = "stardict";

pub struct StardictDatabase {
    conn: Connection,
}

impl StardictDatabase {
    pub fn new(database_path: &str) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_with_flags(
            database_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        AlfredUtils::log(format!("Connected to database (read-only) at {}", database_path));
        Ok(Self { conn })
    }

    pub fn search_word(&self, spell: &str, limit: u32) -> Result<Vec<StardictEntry>, rusqlite::Error> {
        if spell.is_empty() {
            return Ok(Vec::new());
        }
        let limit_i = limit.min(1000);
        let escaped = spell
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("{}%", escaped);
        let sql = format!("
        SELECT id, word, sw, phonetic, definition, translation, pos, collins, oxford, tag, bnc, frq, exchange, detail, audio
        FROM {TABLE}
        WHERE sw LIKE ?1 ESCAPE '\\'
        LIMIT ?2
        ");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params![pattern, limit_i])?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next()? {
            entries.push(row_to_entry(row)?);
        }
        Ok(entries)
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> Result<StardictEntry, rusqlite::Error> {
    Ok(StardictEntry {
        id: row.get(0)?,
        word: row.get(1)?,
        sw: row.get(2)?,
        phonetic: row.get(3)?,
        definition: row.get(4)?,
        translation: row.get(5)?,
        pos: row.get(6)?,
        collins: row.get(7)?,
        oxford: row.get(8)?,
        tag: row.get(9)?,
        bnc: row.get(10)?,
        frq: row.get(11)?,
        exchange: row.get(12)?,
        detail: row.get(13)?,
        audio: row.get(14)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    fn make_fixture() -> NamedTempFile {
        let f = NamedTempFile::new().unwrap();
        let conn = Connection::open(f.path()).unwrap();
        conn.execute_batch("
            CREATE TABLE stardict (id INTEGER PRIMARY KEY, word TEXT, sw TEXT,
              phonetic TEXT, definition TEXT, translation TEXT, pos TEXT,
              collins INTEGER, oxford INTEGER, tag TEXT, bnc INTEGER, frq INTEGER,
              exchange TEXT, detail TEXT, audio TEXT);
            INSERT INTO stardict (word, sw) VALUES ('apple','apple'),('appendix','appendix'),('arc','arc');
        ").unwrap();
        f
    }

    #[test]
    fn prefix_match_finds_apple_and_appendix() {
        let f = make_fixture();
        let db = StardictDatabase::new(f.path().to_str().unwrap()).unwrap();
        let r = db.search_word("app", 10).unwrap();
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn malicious_quote_does_not_inject() {
        let f = make_fixture();
        let db = StardictDatabase::new(f.path().to_str().unwrap()).unwrap();
        let r = db.search_word("a' OR '1'='1", 10).unwrap();
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn literal_percent_does_not_act_as_wildcard() {
        let f = make_fixture();
        let db = StardictDatabase::new(f.path().to_str().unwrap()).unwrap();
        let r = db.search_word("a%", 10).unwrap();
        assert_eq!(r.len(), 0);
    }
}
