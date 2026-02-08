use crate::error::Result;
use rusqlite::Connection;

pub const SCHEMA_VERSION: i32 = 3;

pub struct Schema;

impl Schema {
    /// Initialize database schema
    pub fn initialize(conn: &Connection) -> Result<()> {
        // Performance pragmas
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -64000;
             PRAGMA temp_store = MEMORY;
             PRAGMA mmap_size = 268435456;",
        )?;

        // Create packages table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS packages (
                pkg_id      INTEGER PRIMARY KEY,
                name        TEXT NOT NULL,
                epoch       INTEGER,
                version     TEXT NOT NULL,
                release     TEXT NOT NULL,
                arch        TEXT NOT NULL,
                summary     TEXT NOT NULL,
                description TEXT NOT NULL,
                license     TEXT,
                vcs         TEXT,
                repo        TEXT NOT NULL
            )",
            [],
        )?;

        // Create indexes for common queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_packages_name ON packages(name)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_packages_arch ON packages(arch)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_packages_repo ON packages(repo)",
            [],
        )?;

        // Create requires table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS requires (
                id      INTEGER PRIMARY KEY,
                pkg_id  INTEGER NOT NULL,
                name    TEXT NOT NULL,
                flags   TEXT,
                version TEXT,
                FOREIGN KEY(pkg_id) REFERENCES packages(pkg_id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_requires_pkg_id ON requires(pkg_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_requires_name ON requires(name)",
            [],
        )?;

        // Create provides table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS provides (
                id      INTEGER PRIMARY KEY,
                pkg_id  INTEGER NOT NULL,
                name    TEXT NOT NULL,
                flags   TEXT,
                version TEXT,
                FOREIGN KEY(pkg_id) REFERENCES packages(pkg_id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_provides_pkg_id ON provides(pkg_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_provides_name ON provides(name)",
            [],
        )?;

        // Create directories table (path deduplication for file entries)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS directories (
                dir_id  INTEGER PRIMARY KEY,
                path    TEXT NOT NULL UNIQUE
            )",
            [],
        )?;

        // Create files table (normalized: directory + filename)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                id        INTEGER PRIMARY KEY,
                pkg_id    INTEGER NOT NULL,
                dir_id    INTEGER NOT NULL,
                name      TEXT NOT NULL DEFAULT '',
                file_type INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(pkg_id) REFERENCES packages(pkg_id),
                FOREIGN KEY(dir_id) REFERENCES directories(dir_id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_pkg_id ON files(pkg_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_dir_name ON files(dir_id, name)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_name ON files(name)",
            [],
        )?;

        // Create metadata table for version tracking
        conn.execute(
            "CREATE TABLE IF NOT EXISTS metadata (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        // Set schema version
        conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES ('schema_version', ?)",
            [SCHEMA_VERSION],
        )?;

        Ok(())
    }

    /// Migrate database schema from old version to current.
    /// Should be called before initialize() for existing databases.
    pub fn migrate(conn: &Connection) -> Result<()> {
        let current = Self::get_version(conn).unwrap_or(0);
        if current > 0 && current < SCHEMA_VERSION {
            // v1 -> v2: Replace flat files table with normalized directories + files
            if current < 2 {
                conn.execute_batch(
                    "DROP TABLE IF EXISTS files;
                     DROP INDEX IF EXISTS idx_files_pkg_id;",
                )?;
            }
        }
        Ok(())
    }

    /// Get current schema version
    pub fn get_version(conn: &Connection) -> Result<i32> {
        let version: i32 = conn
            .query_row(
                "SELECT value FROM metadata WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(version)
    }
}
