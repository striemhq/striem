#[cfg(feature = "duckdb")]
pub mod duckdb {
    use crate::sources::Source;
    use anyhow::Result;
    use duckdb::{DuckdbConnectionManager, params};
    use r2d2::PooledConnection;
    use serde::Serialize;
    use serde_json::Value;

    const CREATE_TABLE_SQL: &str = r#"CREATE TABLE IF NOT EXISTS sources (
            id UUID PRIMARY KEY,
            type TEXT,
            config JSON);"#;

    pub fn init(db: &mut PooledConnection<DuckdbConnectionManager>) -> Result<()> {
        db.execute(CREATE_TABLE_SQL, [])?;
        Ok(())
    }
    pub fn add_source(
        db: &mut PooledConnection<DuckdbConnectionManager>,
        source: &Box<dyn Source>,
    ) -> Result<()> {
        let sql = "INSERT INTO sources (type, id, config) VALUES (?, ?, ?)";

        let sourcetype = source.sourcetype().to_string();
        let id = source.id();
        let config = source.config().serialize(serde_json::value::Serializer)?;

        db.prepare(sql)?
            .execute(params![&sourcetype, &id, &config])?;
        Ok(())
    }

    pub fn remove_source(
        db: &mut PooledConnection<DuckdbConnectionManager>,
        id: &String,
    ) -> Result<()> {
        let sql = "DELETE FROM sources WHERE id = ?";
        db.prepare(sql)?.execute(params![&id])?;
        Ok(())
    }

    pub fn sources(
        db: &mut PooledConnection<DuckdbConnectionManager>,
    ) -> Result<Vec<Box<dyn Source>>> {
        let sql = "SELECT type, id, config FROM sources";

        db.prepare(sql)?
            .query([])?
            .mapped(|row| {
                let sourcetype: String = row.get(0)?;
                let id: String = row.get(1)?;
                let config: Value = row.get(2)?;
                Ok((sourcetype, id, config))
            })
            .map(|row| Ok(row?.try_into()))
            .collect::<Result<_, Box<dyn std::error::Error>>>()
            .map_err(|e| anyhow::anyhow!("Failed to fetch sources from database: {}", e))?
    }
}

#[cfg(feature = "duckdb")]
pub use duckdb::*;
