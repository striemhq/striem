use anyhow::Result;
use parquet::schema::{parser::parse_message_type, types::SchemaDescriptor};
use std::{fs, path::PathBuf};

pub fn visit_dirs(path: &PathBuf) -> Result<Vec<(SchemaDescriptor, PathBuf)>> {
    if path.is_dir() {
        let mut schemas = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let entries = match path.is_dir() {
                true => visit_dirs(&path)?,
                false => {
                    let file = std::fs::read_to_string(&path)?;
                    vec![(
                        SchemaDescriptor::new(parse_message_type(file.as_str())?.into()),
                        path,
                    )]
                }
            };
            schemas.extend(entries);
        }
        Ok(schemas)
    } else {
        let file = std::fs::read_to_string(path)?;
        Ok(vec![(
            SchemaDescriptor::new(parse_message_type(file.as_str())?.into()),
            path.to_path_buf(),
        )])
    }
}
