mod convert;
//mod proto;

mod client;
mod server;

#[allow(unused)]
pub mod event {
    include!(concat!(env!("OUT_DIR"), "/proto/event.rs"));
}

#[allow(unused)]
pub mod vector {
    include!(concat!(env!("OUT_DIR"), "/proto/vector.rs"));
}

pub use client::Client;
pub use server::Server;
