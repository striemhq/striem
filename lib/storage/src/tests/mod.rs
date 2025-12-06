use std::{
    fs::{File, remove_file},
    io::Read,
    sync::Arc,
};

use parquet::{
    arrow::{ArrowWriter, parquet_to_arrow_schema},
    file::reader::{FileReader, SerializedFileReader},
    schema::{parser::parse_message_type, types::SchemaDescriptor},
};

use tokio;

use serde_json::json;

use super::*;

const SCHEMA: &str = r#"message api_activity {
    optional INT32 activity_id (INTEGER(32, true));
    optional BYTE_ARRAY activity_name (STRING);
    optional group actor {
        optional BYTE_ARRAY app_name (STRING);
    }
    optional group authorizations (LIST) {
        repeated group list {
        optional BYTE_ARRAY decision (STRING);
        optional BOOLEAN is_applied;
        }
    }
    }"#;

#[test]
fn parquet_test() {
    let temp_path = format!(
        "{}/{}.test.parquet",
        std::env::temp_dir().display(),
        std::process::id()
    );
    let mut parquet_file = File::create(temp_path.clone()).unwrap();
    let input = json!({
        "activity_id": 1,
        "activity_name": "test",
        "actor": {
            "app_name": "test"
        },
        "authorizations": [
            {
                "decision": "test",
                "is_applied": true
            }
        ]
    });
    let parquet_schema = SchemaDescriptor::new(parse_message_type(SCHEMA).unwrap().into());
    let arrow_schema = Arc::new(parquet_to_arrow_schema(&parquet_schema, None).unwrap());
    let record_batch = convert_json(&input, &arrow_schema).unwrap();

    let props = parquet::file::properties::WriterProperties::builder()
        .set_writer_version(parquet::file::properties::WriterVersion::PARQUET_2_0)
        .set_compression(parquet::basic::Compression::SNAPPY)
        .build();

    let mut writer = ArrowWriter::try_new(&mut parquet_file, arrow_schema, Some(props)).unwrap();
    writer.write(&record_batch).unwrap();
    writer.close().unwrap();

    let parquet_file = File::open(temp_path.clone()).unwrap();
    let reader = SerializedFileReader::new(parquet_file).unwrap();

    let v = reader
        .get_row_group(0)
        .unwrap()
        .get_row_iter(None)
        .unwrap()
        .map(|r| {
            let r = r.unwrap();
            r.to_json_value()
        })
        .collect::<Vec<_>>();

    remove_file(temp_path).unwrap();

    assert_eq!(v[0], input);
}

use super::writer::Writer;
#[tokio::test]
async fn writer_test() {
    let temp_path = format!("{}/{}", std::env::temp_dir().display(), std::process::id());
    tokio::fs::create_dir_all(&temp_path).await.unwrap();

    let input = json!({
        "activity_id": 1,
        "activity_name": "test",
        "actor": {
            "app_name": "test"
        },
        "authorizations": [
            {
                "decision": "test",
                "is_applied": true
            }
        ]
    });

    let parquet_schema = SchemaDescriptor::new(parse_message_type(SCHEMA).unwrap().into());
    let arrow_schema = Arc::new(parquet_to_arrow_schema(&parquet_schema, None).unwrap());

    let record_batch = convert_json(&input, &arrow_schema).unwrap();

    let writer = Writer::new(temp_path.clone(), arrow_schema).await.unwrap();

    writer.write(&record_batch).await.unwrap();
    drop(writer);

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let path = std::fs::read_dir(temp_path.clone())
        .unwrap()
        .filter_map(Result::ok)
        .find(|p| p.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .map(|p| p.path())
        .expect("No files found in directory");

    let parquet_file = File::open(path).unwrap();
    let reader = SerializedFileReader::new(parquet_file).unwrap();

    let v = reader
        .get_row_group(0)
        .unwrap()
        .get_row_iter(None)
        .unwrap()
        .map(|r| {
            let r = r.unwrap();
            r.to_json_value()
        })
        .collect::<Vec<_>>();

    assert_eq!(v[0], input);
}
