use std::collections::HashMap;
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

struct Shell {
    variables: HashMap<String, String>,
}

impl Shell {
    fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    fn set_variable(&mut self, name: &str, value: &str) {
        self.variables.insert(name.to_string(), value.to_string());
    }

    fn get_variable(&self, name: &str) -> Option<&String> {
        self.variables.get(name)
    }

    fn expand(&self, input: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '$' {
                let mut name = String::new();
                let mut j = i + 1;

                while j < chars.len() {
                    let c = chars[j];

                    if c.is_alphanumeric() || c == '_' {
                        name.push(c);
                        j += 1;
                    } else {
                        break;
                    }
                }

                if !name.is_empty() {
                    if let Some(value) = self.get_variable(&name) {
                        result.push_str(value);
                    } else {
                        let variable = format!("${name}");
                        result.push_str(&expand_environment_variable(&variable));
                    }

                    i = j;
                    continue;
                }
            }

            result.push(chars[i]);
            i += 1;
        }

        result
    }
}

fn expand_environment_variable(variable: &str) -> String {
    match variable {
        "$HOME" => env::var("HOME").unwrap_or_default(),
        "$USER" => env::var("USER").unwrap_or_default(),
        "$SHELL" => env::var("SHELL").unwrap_or_default(),
        "$OLDPWD" => env::var("OLDPWD").unwrap_or_default(),
        "$LANG" => env::var("LANG").unwrap_or_default(),
        "$TERM" => env::var("TERM").unwrap_or_default(),
        "$EDITOR" => env::var("EDITOR").unwrap_or_default(),
        "$VISUAL" => env::var("VISUAL").unwrap_or_default(),
        "$HOSTNAME" => env::var("HOSTNAME").unwrap_or_default(),
        "$LOGNAME" => env::var("LOGNAME").unwrap_or_default(),
        "$TMPDIR" => env::var("TMPDIR").unwrap_or_default(),
        "$XDG_CONFIG_HOME" => env::var("XDG_CONFIG_HOME").unwrap_or_default(),
        "$XDG_DATA_HOME" => env::var("XDG_DATA_HOME").unwrap_or_default(),
        "$XDG_CACHE_HOME" => env::var("XDG_CACHE_HOME").unwrap_or_default(),
        "$XDG_RUNTIME_DIR" => env::var("XDG_RUNTIME_DIR").unwrap_or_default(),
        "$PWD" => env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        _ => variable.to_string(),
    }
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut escape = false;
    let mut single_quoted = false;
    let mut double_quoted = false;

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if escape {
            current.push(c);
            escape = false;
            i += 1;
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

            '&' if !double_quoted && !single_quoted => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }

                if i + 1 < chars.len() && chars[i + 1] == '&' {
                    tokens.push("&&".to_string());
                    i += 1;
                } else {
                    current.push('&');
                }
            }

            '|' if !double_quoted && !single_quoted => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }

                if i + 1 < chars.len() && chars[i + 1] == '|' {
                    tokens.push("||".to_string());
                    i += 1;
                } else {
                    tokens.push("|".to_string());
                }
            }

            '>' if !double_quoted && !single_quoted => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }

                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    tokens.push(">>".to_string());
                    i += 1;
                } else {
                    tokens.push(">".to_string());
                }
            }

            '<' if !double_quoted && !single_quoted => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }

                tokens.push("<".to_string());
            }

            ';' if !double_quoted && !single_quoted => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }

                tokens.push(";".to_string());
            }

            _ => {
                current.push(c);
            }
        }

        i += 1;
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

fn execute_pipeline(shell: &Shell, commands: &[Vec<&str>]) -> bool {
    if commands.is_empty() {
        return false;
    }

    let mut specs = Vec::new();

    for command_tokens in commands {
        match parse_command(command_tokens.clone()) {
            Some(spec) => specs.push(spec),
            None => return false,
        }
    }

    if specs.is_empty() {
        return false;
    }

    if specs.len() == 1 && specs[0].command == "cd" {
        if let Some(path) = specs[0].args.first() {
            let path = shell.expand(path);

            let oldpwd = match env::current_dir() {
                Ok(path) => path,
                Err(e) => {
                    eprintln!("cd: {e}");
                    return false;
                }
            };

            if let Err(e) = env::set_current_dir(path) {
                eprintln!("cd: {e}");
                return false;
            }

            unsafe {
                env::set_var("OLDPWD", oldpwd);
            }
        } else if let Ok(home) = env::var("HOME") {
            let oldpwd = match env::current_dir() {
                Ok(path) => path,
                Err(e) => {
                    eprintln!("cd: {e}");
                    return false;
                }
            };

            if let Err(e) = env::set_current_dir(home) {
                eprintln!("cd: {e}");
                return false;
            }

            unsafe {
                env::set_var("OLDPWD", oldpwd);
            }
        }

        return true;
    }

    let mut children: Vec<Child> = Vec::new();
    let mut previous_stdout = None;
    let mut success = true;

    for (index, spec) in specs.iter().enumerate() {
        let is_first = index == 0;
        let is_last = index == specs.len() - 1;

        let mut process = Command::new(shell.expand(spec.command));

        let expanded_args: Vec<String> = spec
            .args
            .iter()
            .map(|arg| shell.expand(arg))
            .collect();

        process.args(&expanded_args);

        if let Some(path) = spec.input {
            let path = shell.expand(path);

            match File::open(path) {
                Ok(file) => {
                    process.stdin(Stdio::from(file));
                }
                Err(e) => {
                    eprintln!("rc9: {e}");
                    return false;
                }
            }
        } else if let Some(stdout) = previous_stdout.take() {
            process.stdin(Stdio::from(stdout));
        } else if !is_first {
            process.stdin(Stdio::null());
        }

        if let Some(path) = spec.output {
            let path = shell.expand(path);

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
                        return false;
                    }
                }
            } else {
                match File::create(path) {
                    Ok(file) => {
                        process.stdout(Stdio::from(file));
                    }
                    Err(e) => {
                        eprintln!("rc9: {e}");
                        return false;
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
                return false;
            }
        }
    }

    for mut child in children {
        match child.wait() {
            Ok(status) => {
                if !status.success() {
                    success = false;
                }
            }

            Err(e) => {
                eprintln!("rc9: {e}");
                success = false;
            }
        }
    }

    success
}

fn main() {
    let home = env::var("HOME").unwrap();

    env::set_current_dir(home).unwrap();

    let mut shell = Shell::new();

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

        if tokens.len() == 1 && tokens[0].contains('=') {
            let parts: Vec<&str> = tokens[0].splitn(2, '=').collect();

            if parts.len() == 2 && !parts[0].is_empty() {
                shell.set_variable(parts[0], parts[1]);
                continue;
            }
        }

        let mut commands: Vec<Vec<Vec<&str>>> = Vec::new();
        let mut operators: Vec<&str> = Vec::new();

        let mut pipeline: Vec<Vec<&str>> = Vec::new();
        let mut current: Vec<&str> = Vec::new();

        let mut invalid = false;

        for token in &tokens {
            match token.as_str() {
                "|" => {
                    if current.is_empty() {
                        eprintln!("rc9: invalid pipe");
                        invalid = true;
                        break;
                    }

                    pipeline.push(current);
                    current = Vec::new();
                }

                "&&" | "||" | ";" => {
                    if current.is_empty() {
                        eprintln!("rc9: invalid command");
                        invalid = true;
                        break;
                    }

                    pipeline.push(current);
                    current = Vec::new();

                    commands.push(pipeline);
                    pipeline = Vec::new();

                    operators.push(token.as_str());
                }

                _ => {
                    current.push(token.as_str());
                }
            }
        }

        if invalid {
            continue;
        }

        if !current.is_empty() {
            pipeline.push(current);
        } else if !pipeline.is_empty() {
            eprintln!("rc9: invalid pipe");
            continue;
        }

        if !pipeline.is_empty() {
            commands.push(pipeline);
        }

        if commands.len() != operators.len() + 1 {
            eprintln!("rc9: invalid command chain");
            continue;
        }

        let mut previous_status = true;

        for (index, command) in commands.iter().enumerate() {
            if index > 0 {
                let operator = operators[index - 1];

                match operator {
                    "&&" if !previous_status => {
                        continue;
                    }

                    "||" if previous_status => {
                        continue;
                    }

                    _ => {}
                }
            }

            previous_status = execute_pipeline(&shell, command);
        }
    }
}
