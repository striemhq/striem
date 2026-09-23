use rsigma_eval::LogSourceExtractor;
use crate::event::LogsourceEvent;

pub struct VectorLogSourceExtractor;


impl VectorLogSourceExtractor {
    fn ls_extract(&self, event: &LogsourceEvent) -> rsigma_parser::LogSource {
        // Implement the actual extraction logic here
        rsigma_parser::LogSource::default()
    }
}

impl<'a> LogSourceExtractor<LogsourceEvent<'a>> for VectorLogSourceExtractor
{
    fn extract(&self, event: &LogsourceEvent<'a>) -> rsigma_parser::LogSource {
        self.ls_extract(event)
    }
    fn resolve(&self, event: &LogsourceEvent<'a>, field: &str, default: Option<&str>) -> Option<String> {
        self.ls_extract(event).custom.get(field).cloned().or_else(|| default.map(str::to_string))
    }
}
