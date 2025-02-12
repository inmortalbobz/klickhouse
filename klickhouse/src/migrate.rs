use crate::{query_parser, FromSql};
use async_trait::async_trait;
use refinery_core::traits::r#async::{AsyncMigrate, AsyncQuery, AsyncTransaction};
use refinery_core::Migration;
use std::borrow::Cow;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::{Client, KlickhouseError, Result, Row, Type, Value};

/// copied from refinery_core
#[allow(dead_code)]
enum State {
    Applied,
    Unapplied,
}

/// copied from refinery_core
#[allow(dead_code)]
enum TypeInner {
    Versioned,
    Unversioned,
}

/// copied from refinery_core
#[allow(dead_code)]
struct MigrationInner {
    state: State,
    name: String,
    checksum: u64,
    version: i32,
    prefix: TypeInner,
    sql: Option<String>,
    applied_on: Option<OffsetDateTime>,
}

impl MigrationInner {
    fn applied(
        version: i32,
        name: String,
        applied_on: OffsetDateTime,
        checksum: u64,
    ) -> MigrationInner {
        MigrationInner {
            state: State::Applied,
            name,
            checksum,
            version,
            // applied migrations are always versioned
            prefix: TypeInner::Versioned,
            sql: None,
            applied_on: Some(applied_on),
        }
    }
}

impl Into<Migration> for MigrationInner {
    fn into(self) -> Migration {
        assert_eq!(
            std::mem::size_of::<Migration>(),
            std::mem::size_of::<MigrationInner>()
        );
        unsafe { std::mem::transmute(self) }
    }
}

impl Row for Migration {
    fn deserialize_row(map: Vec<(&str, &Type, Value)>) -> Result<Self> {
        if map.len() != 4 {
            return Err(KlickhouseError::DeserializeError(
                "bad column count for migration".to_string(),
            ));
        }
        let mut version = None::<i32>;
        let mut name_out = None::<String>;
        let mut applied_on = None::<String>;
        let mut checksum = None::<String>;
        for (name, type_, value) in map {
            match name {
                "version" => {
                    if version.is_some() {
                        return Err(KlickhouseError::DeserializeError(
                            "duplicate version column".to_string(),
                        ));
                    }
                    version = Some(i32::from_sql(type_, value)?);
                }
                "name" => {
                    if name_out.is_some() {
                        return Err(KlickhouseError::DeserializeError(
                            "duplicate name column".to_string(),
                        ));
                    }
                    name_out = Some(String::from_sql(type_, value)?);
                }
                "applied_on" => {
                    if applied_on.is_some() {
                        return Err(KlickhouseError::DeserializeError(
                            "duplicate applied_on column".to_string(),
                        ));
                    }
                    applied_on = Some(String::from_sql(type_, value)?);
                }
                "checksum" => {
                    if checksum.is_some() {
                        return Err(KlickhouseError::DeserializeError(
                            "duplicate checksum column".to_string(),
                        ));
                    }
                    checksum = Some(String::from_sql(type_, value)?);
                }
                name => {
                    return Err(KlickhouseError::DeserializeError(format!(
                        "unexpected column {name}"
                    )));
                }
            }
        }
        if version.is_none() {
            return Err(KlickhouseError::DeserializeError(
                "missing version".to_string(),
            ));
        }
        if name_out.is_none() {
            return Err(KlickhouseError::DeserializeError(
                "missing name".to_string(),
            ));
        }
        if applied_on.is_none() {
            return Err(KlickhouseError::DeserializeError(
                "missing applied_on".to_string(),
            ));
        }
        if checksum.is_none() {
            return Err(KlickhouseError::DeserializeError(
                "missing checksum".to_string(),
            ));
        }
        let applied_on =
            OffsetDateTime::parse(applied_on.as_ref().unwrap(), &Rfc3339).map_err(|e| {
                KlickhouseError::DeserializeError(format!("failed to parse time: {:?}", e))
            })?;

        Ok(MigrationInner::applied(
            version.unwrap(),
            name_out.unwrap(),
            applied_on,
            checksum.unwrap().parse::<u64>().map_err(|e| {
                KlickhouseError::DeserializeError(format!("failed to parse checksum: {:?}", e))
            })?,
        )
        .into())
    }

    fn serialize_row(self) -> Result<Vec<(Cow<'static, str>, Value)>> {
        unimplemented!()
    }
}

#[async_trait]
impl AsyncTransaction for Client {
    type Error = KlickhouseError;

    async fn execute(&mut self, queries: &[&str]) -> Result<usize, Self::Error> {
        for query in queries {
            for query in query_parser::split_query_statements(query) {
                Client::execute(self, query).await?;
            }
        }
        Ok(queries.len())
    }
}

#[async_trait]
impl AsyncQuery<Vec<Migration>> for Client {
    async fn query(
        &mut self,
        query: &str,
    ) -> Result<Vec<Migration>, <Self as AsyncTransaction>::Error> {
        self.query_collect::<Migration>(query).await
    }
}

impl AsyncMigrate for Client {
    fn assert_migrations_table_query(migration_table_name: &str) -> String {
        format!(
            "CREATE TABLE IF NOT EXISTS {migration_table_name}(
            version INT,
            name VARCHAR(255),
            applied_on VARCHAR(255),
            checksum VARCHAR(255)) Engine=MergeTree() ORDER BY version;"
        )
    }
}
