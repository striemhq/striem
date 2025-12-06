#[cfg(test)]
use super::*;

#[test]
fn test_read_config() {
    let config = r#"
      detections:
        - /path/to/sigmarules
        - /path/to/more/rules
      input:
        vector:
          address: 0.0.0.0:50050
      output:
          vector:
            url: http://127.0.0.1:6000
      storage:
        schema: ocsf/schema
        path: data/ocsf
    "#;
    let config = StrIEMConfig::from_yaml(config).unwrap();

    assert_eq!(
        config.detections,
        Some(StringOrList::List(vec![
            "/path/to/sigmarules".into(),
            "/path/to/more/rules".into()
        ]))
    );
}
/*
#[test]
fn test_env() {
    std::env::set_var("STRIEM_SOURCE_VECTOR_ADDRESS", "1.2.3.4:1234");
    std::env::set_var("STRIEM_DETECTIONS", "/path/to/sigmarules");
    std::env::set_var("STRIEM_OUTPUT_VECTOR_ADDRESS", "1.2.3.4:1234");
    std::env::set_var("STRIEM_STORAGE_SCHEMA", "ocsf/schema");
    std::env::set_var("STRIEM_STORAGE_PATH", "data/ocsf");
    let cfg = StrIEMConfig::default();
    println!("{:?}", cfg);
}
*/
