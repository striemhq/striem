#[cfg(test)]
use super::*;

#[test]
fn test_read_config() {
    let config = r#"
      detections:
        - /path/to/sigmarules
        - /path/to/more/rules
      input:
        address: 0.0.0.0:50050
      output:
        url: http://127.0.0.1:6000
      storage:
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
    assert_eq!(config.input.address().to_string(), "0.0.0.0:50050");
    assert_eq!(
        config.output.map(|o| o.cfg.url()),
        Some("http://127.0.0.1:6000/".to_string())
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
