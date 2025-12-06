//mod buffer;
mod backend;
mod convert;
mod util;
mod writer;

mod ocsf {
    include!(concat!(env!("OUT_DIR"), "/ocsf.rs"));
}

pub use crate::backend::ParquetBackend;
pub use convert::convert_json;
pub use writer::Writer;

#[cfg(test)]
mod tests;
