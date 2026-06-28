//! J-ALERT receiver core: model, classifier, shared state, upstream source and
//! the embedded web/cloudflared layer. The GUI lives in the binary (`main.rs`).

pub mod classify;
pub mod model;
pub mod source;
pub mod state;
pub mod web;
