//! Language reference CLI formatting

use lumen_compiler::lang_ref;

pub fn run(json: bool) {
    let reference = lang_ref::generate();
    if json {
        println!("{}", serde_json::to_string_pretty(&reference).unwrap());
    } else {
        println!("{}", lang_ref::format_markdown(&reference));
    }
}
