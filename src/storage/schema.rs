use crate::error::Result;
use rusqlite::Connection;

pub const SCHEMA_VERSION: i32 = 1;

pub struct Schema;

impl Schema {
    /// Initialize database schema
    pub fn initialize(conn: &Connection) -> Result<()> {
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

        // Create files table (optional, can be disabled for large repos)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                id      INTEGER PRIMARY KEY,
                pkg_id  INTEGER NOT NULL,
                path    TEXT NOT NULL,
                FOREIGN KEY(pkg_id) REFERENCES packages(pkg_id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_pkg_id ON files(pkg_id)",
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

    /// Get current schema version
    #[allow(dead_code)]
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
