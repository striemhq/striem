use std::{fs, path::Path};

use parquet::schema::{parser::parse_message_type, types::SchemaDescriptor};

pub fn visit_dirs(
    dir: &Path,
) -> Result<Vec<(SchemaDescriptor, String)>, Box<dyn std::error::Error>> {
    if dir.is_dir() {
        let mut schemas = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let entries = match path.is_dir() {
                true => visit_dirs(&path)?,
                false => {
                    let file = std::fs::read_to_string(&path)?;
                    vec![(
                        SchemaDescriptor::new(parse_message_type(file.as_str())?.into()),
                        path.to_string_lossy().into_owned(),
                    )]
                }
            };
            schemas.extend(entries);
        }
        Ok(schemas)
    } else {
        let file = std::fs::read_to_string(dir)?;
        Ok(vec![(
            SchemaDescriptor::new(parse_message_type(file.as_str())?.into()),
            dir.to_string_lossy().into_owned(),
        )])
    }
}
