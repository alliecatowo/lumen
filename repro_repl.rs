
// Mock REPL implementation logic from repl.rs to test state persistence
use std::collections::HashMap;

#[derive(Default)]
struct SessionState {
    definitions: Vec<String>,
    symbols: HashMap<String, usize>,
}

impl SessionState {
    fn add_definition(&mut self, input: &str) {
        self.definitions.push(input.to_string());
    }

    fn build_source(&self, input: &str) -> String {
        let mut src = String::from("  // repl\n");
        for def in &self.definitions {
            src.push_str(def);
            src.push('\n');
        }
        
        // Logic from repl.rs wrap_as_source
        if input.starts_with("let ") {
             src.push_str(input);
        } else {
             src.push_str(&format!("cell main()\n  print({})\nend", input));
        }
        src
    }
}

fn main() {
    let mut session = SessionState::default();
    
    // Line 1: let x = 10
    let line1 = "let x = 10";
    session.add_definition(line1);
    
    // Line 2: print(x) -> wrapped as cell main() print(x) end
    let line2 = "x";
    let source = session.build_source(line2);
    
    println!("--- Generated Source ---\n{}", source);
}
