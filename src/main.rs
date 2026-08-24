use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::process::{Child, Command, Stdio};

struct CommandSpec<'a> {
    command: &'a str,
    args: Vec<&'a str>,
    input: Option<&'a str>,
    output: Option<&'a str>,
    append: bool,
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut escape = false;
    let mut single_quoted = false;
    let mut double_quoted = false;

    for c in input.chars() {
        if escape {
            current.push(c);
            escape = false;
            continue;
        }

        match c {
            '"' => {
                if !single_quoted {
                    double_quoted = !double_quoted;
                } else {
                    current.push(c);
                }
            }
            '\\' => {
                escape = true;
            }
            '\'' => {
                if !double_quoted {
                    single_quoted = !single_quoted;
                } else {
                    current.push(c);
                }
            }
            c if c.is_whitespace() && !double_quoted && !single_quoted => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            '|' if !double_quoted && !single_quoted => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                tokens.push("|".to_string());
            }
            '>' if !double_quoted && !single_quoted => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                tokens.push(">".to_string());
            }
            '<' if !double_quoted && !single_quoted => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                tokens.push("<".to_string());
            }
            _ => current.push(c),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn parse_command<'a>(tokens: Vec<&'a str>) -> Option<CommandSpec<'a>> {
    let command = tokens.first()?;

    let mut spec = CommandSpec {
        command,
        args: tokens[1..].to_vec(),
        input: None,
        output: None,
        append: false,
    };

    let mut i = 0;

    while i < spec.args.len() {
        if spec.args[i] == "<" {
            if i + 1 >= spec.args.len() {
                eprintln!("rc9: expected file after '<'");
                return None;
            }

            spec.input = Some(spec.args[i + 1]);
            spec.args.remove(i + 1);
            spec.args.remove(i);
        } else if spec.args[i] == ">" {
            if i + 1 >= spec.args.len() {
                eprintln!("rc9: expected file after '>'");
                return None;
            }

            spec.output = Some(spec.args[i + 1]);
            spec.append = false;
            spec.args.remove(i + 1);
            spec.args.remove(i);
        } else if spec.args[i] == ">>" {
            if i + 1 >= spec.args.len() {
                eprintln!("rc9: expected file after '>>'");
                return None;
            }

            spec.output = Some(spec.args[i + 1]);
            spec.append = true;
            spec.args.remove(i + 1);
            spec.args.remove(i);
        } else {
            i += 1;
        }
    }

    Some(spec)
}

fn main() {
    let home = env::var("HOME").unwrap();
    env::set_current_dir(home).unwrap();

    loop {
        let dir = env::current_dir().unwrap();

        print!("{}% ", dir.display());
        io::stdout().flush().unwrap();

        let mut input = String::new();

        if io::stdin().read_line(&mut input).is_err() {
            break;
        }

        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input == "exit" {
            break;
        }

        let tokens = tokenize(input);

        let mut commands: Vec<Vec<&str>> = Vec::new();
        let mut current = Vec::new();
        let mut invalid_pipe = false;

        for token in &tokens {
            if token == "|" {
                if current.is_empty() {
                    eprintln!("rc9: invalid pipe");
                    invalid_pipe = true;
                    break;
                }

                commands.push(current);
                current = Vec::new();
            } else {
                current.push(token.as_str());
            }
        }

        if invalid_pipe {
            continue;
        }

        if current.is_empty() {
            if !commands.is_empty() {
                eprintln!("rc9: invalid pipe");
                continue;
            }
        } else {
            commands.push(current);
        }

        if commands.is_empty() {
            continue;
        }

        let mut specs = Vec::new();

        for command_tokens in commands {
            match parse_command(command_tokens) {
                Some(spec) => specs.push(spec),
                None => {
                    specs.clear();
                    break;
                }
            }
        }

        if specs.is_empty() {
            continue;
        }

        if specs.len() == 1 && specs[0].command == "cd" {
            if let Some(path) = specs[0].args.first() {
                if let Err(e) = env::set_current_dir(path) {
                    eprintln!("cd: {e}");
                }
            } else if let Ok(home) = env::var("HOME") {
                if let Err(e) = env::set_current_dir(home) {
                    eprintln!("cd: {e}");
                }
            }

            continue;
        }

        let mut children: Vec<Child> = Vec::new();
        let mut previous_stdout = None;

        for (index, spec) in specs.iter().enumerate() {
            let is_first = index == 0;
            let is_last = index == specs.len() - 1;

            let mut process = Command::new(spec.command);
            process.args(&spec.args);

            if let Some(path) = spec.input {
                match File::open(path) {
                    Ok(file) => {
                        process.stdin(Stdio::from(file));
                    }
                    Err(e) => {
                        eprintln!("rc9: {e}");
                        continue;
                    }
                }
            } else if let Some(stdout) = previous_stdout.take() {
                process.stdin(Stdio::from(stdout));
            } else if !is_first {
                process.stdin(Stdio::null());
            }

            if let Some(path) = spec.output {
                if spec.append {
                    match OpenOptions::new()
                        .write(true)
                        .append(true)
                        .create(true)
                        .open(path)
                    {
                        Ok(file) => {
                            process.stdout(Stdio::from(file));
                        }
                        Err(e) => {
                            eprintln!("rc9: {e}");
                            continue;
                        }
                    }
                } else {
                    match File::create(path) {
                        Ok(file) => {
                            process.stdout(Stdio::from(file));
                        }
                        Err(e) => {
                            eprintln!("rc9: {e}");
                            continue;
                        }
                    }
                }
            } else if !is_last {
                process.stdout(Stdio::piped());
            }

            match process.spawn() {
                Ok(mut child) => {
                    if !is_last && spec.output.is_none() {
                        previous_stdout = child.stdout.take();
                    }

                    children.push(child);
                }
                Err(e) => {
                    eprintln!("rc9: {}: {}", spec.command, e);
                }
            }
        }

        for mut child in children {
            match child.wait() {
                Ok(status) => {
                    println!("exit: {status}");
                }
                Err(e) => {
                    eprintln!("rc9: {e}");
                }
            }
        }
    }
}
