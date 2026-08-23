use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::process::Command;

struct CommandSpec<'a> {
    command: &'a str,
    args: Vec<&'a str>,
    output: Option<&'a str>,
    append: bool,
}

fn main() {
    let home = env::var("HOME").unwrap();
    env::set_current_dir(home).unwrap();

    loop {
        let dir = env::current_dir().unwrap();

        print!("{}% ", dir.display());
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input == "exit" {
            break;
        }

        let mut parts = input.split_whitespace();

        let command = match parts.next() {
            Some(cmd) => cmd,
            None => continue,
        };

        let mut spec = CommandSpec {
            command,
            args: parts.collect(),
            output: None,
            append: false,
        };

        let mut i = 0;

        while i < spec.args.len() {
            if spec.args[i] == ">" {
                if i + 1 >= spec.args.len() {
                    eprintln!("rc9: expected file after '>'");
                    break;
                }

                spec.output = Some(spec.args[i + 1]);
                spec.append = false;

                spec.args.remove(i + 1);
                spec.args.remove(i);
            } else if spec.args[i] == ">>" {
                if i + 1 >= spec.args.len() {
                    eprintln!("rc9: expected file after '>>'");
                    break;
                }

                spec.output = Some(spec.args[i + 1]);
                spec.append = true;

                spec.args.remove(i + 1);
                spec.args.remove(i);
            } else {
                i += 1;
            }
        }

        if spec.command == "cd" {
            if let Some(path) = spec.args.first() {
                if let Err(e) = env::set_current_dir(path) {
                    eprintln!("cd: {e}");
                }
            }

            continue;
        }

        let mut process = Command::new(spec.command);
        process.args(&spec.args);

        if let Some(path) = spec.output {
            if spec.append {
                match OpenOptions::new()
                    .write(true)
                    .append(true)
                    .create(true)
                    .open(path)
                {
                    Ok(file) => {
                        process.stdout(file);
                    }
                    Err(e) => {
                        eprintln!("rc9: {e}");
                        continue;
                    }
                }
            } else {
                match File::create(path) {
                    Ok(file) => {
                        process.stdout(file);
                    }
                    Err(e) => {
                        eprintln!("rc9: {e}");
                        continue;
                    }
                }
            }
        }

        match process.status() {
            Ok(status) => println!("exit: {status}"),
            Err(e) => eprintln!("rc9: {e}"),
        }
    }
}
