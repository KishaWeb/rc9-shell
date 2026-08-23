use std::io::{self, Write};

fn main() {
    loop {
        print!("rune> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();

        io::stdin()
            .read_line(&mut input)
            .unwrap();

        let input = input.trim();

        if input == "exit" {
            break;
        }

        if input.is_empty() {
            continue;
        }

        println!("You entered: {input}");
    }
}
