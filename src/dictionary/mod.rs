pub mod database;
pub mod entry;
pub mod manager;
mod completion_words;

pub use entry::StardictEntry;
pub use manager::{DictionaryConfig, DictionaryManager};
