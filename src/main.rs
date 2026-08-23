use std::io::{self, Write};
use std::process::Command;
use std::env::{self};

fn main() {
    
    let home = env::var("HOME").unwrap();
    env::set_current_dir(home).unwrap();
    
    loop {        
        let dir = env::current_dir().unwrap();
        print!("{}% ",dir.display());
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        let input = input.trim();
        
        let mut parts = input.split_whitespace();
        let command = match parts.next() {
            Some(cmd) => cmd,
            None => continue,
        };

        let args: Vec<&str> = parts.collect();
        
        if input == "exit" {
            break;
        }else if command == "cd" {
            if let Some(path) = args.first() {
                if let Err(e) = env::set_current_dir(path) {
                    eprintln!("cd: {e}");
                }
            }
            continue;
        }
                
        match Command::new(command).args(args).status() {
            Ok(status) => println!("exit: {status}"),
            Err(e) => eprintln!("error: {e}"),
        }
    }
}
