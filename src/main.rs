use std::collections::HashMap;
use std::env;
use std::fs::{File, OpenOptions};
use std::process::{Child, Command, Stdio};

use rustyline::error::ReadlineError;

struct CommandSpec<'a> {
    command: &'a str,
    args: Vec<&'a str>,
    input: Option<&'a str>,
    output: Option<&'a str>,
    append: bool,
}

struct Shell {
    variables: HashMap<String, Vec<String>>,
}

impl Shell {
    fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    fn set_variable(&mut self, name: &str, value: Vec<String>) {
        self.variables.insert(name.to_string(), value);
    }

    fn get_variable(&self, name: &str) -> Option<&Vec<String>> {
        self.variables.get(name)
    }

    fn expand(&self, input: &str) -> Vec<String> {
        if input.starts_with('$') {
            let name = &input[1..];

            if let Some(value) = self.get_variable(name) {
                return value.clone();
            }

            return vec![expand_environment_variable(input)];
        }

        vec![input.to_string()]
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

            '(' if !double_quoted && !single_quoted => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }

                tokens.push("(".to_string());
            }

            ')' if !double_quoted && !single_quoted => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }

                tokens.push(")".to_string());
            }

            '=' if !double_quoted && !single_quoted => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }

                tokens.push("=".to_string());
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
            let expanded = shell.expand(path);

            let Some(path) = expanded.first() else {
                return false;
            };

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

        let expanded_command = shell.expand(spec.command);

        let Some(command) = expanded_command.first() else {
            return false;
        };

        let mut process = Command::new(command);

        let expanded_args: Vec<String> = spec
            .args
            .iter()
            .flat_map(|arg| shell.expand(arg))
            .collect();

        process.args(&expanded_args);

        if let Some(path) = spec.input {
            let expanded = shell.expand(path);

            let Some(path) = expanded.first() else {
                return false;
            };

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
            let expanded = shell.expand(path);

            let Some(path) = expanded.first() else {
                return false;
            };

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

fn needs_more_input(input: &str) -> bool {
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut parentheses = 0;
    let mut escape = false;

    for c in input.chars() {
        if escape {
            escape = false;
            continue;
        }

        match c {
            '\\' => {
                escape = true;
            }

            '"' if !single_quoted => {
                double_quoted = !double_quoted;
            }

            '\'' if !double_quoted => {
                single_quoted = !single_quoted;
            }

            '(' if !single_quoted && !double_quoted => {
                parentheses += 1;
            }

            ')' if !single_quoted && !double_quoted && parentheses > 0 => {
                parentheses -= 1;
            }

            _ => {}
        }
    }

    single_quoted || double_quoted || parentheses > 0
}

fn execute_input(shell: &mut Shell, input: &str) -> bool {
    let tokens = tokenize(input);

    if tokens.is_empty() {
        return true;
    }

    if tokens.len() >= 3 && tokens[1] == "=" {
        let name = tokens[0].as_str();

        if tokens[2] == "(" {
            let mut values = Vec::new();
            let mut i = 3;

            while i < tokens.len() && tokens[i] != ")" {
                values.push(tokens[i].clone());
                i += 1;
            }

            if i == tokens.len() {
                eprintln!("rc9: expected ')'");
                return false;
            }

            shell.set_variable(name, values);
            return true;
        }

        shell.set_variable(name, vec![tokens[2..].join(" ")]);
        return true;
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
        return false;
    }

    if !current.is_empty() {
        pipeline.push(current);
    } else if !pipeline.is_empty() {
        eprintln!("rc9: invalid pipe");
        return false;
    }

    if !pipeline.is_empty() {
        commands.push(pipeline);
    }

    if commands.len() != operators.len() + 1 {
        eprintln!("rc9: invalid command chain");
        return false;
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

        previous_status = execute_pipeline(shell, command);
    }

    previous_status
}

fn load_rc9rc(shell: &mut Shell) {
    let Ok(home) = env::var("HOME") else {
        return;
    };

    let path = format!("{home}/.rc9rc");

    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
    };

    for line in contents.lines() {
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        execute_input(shell, line);
    }
}

fn main() {
    let home = env::var("HOME").unwrap();

    env::set_current_dir(home).unwrap();

    let mut shell = Shell::new();

    load_rc9rc(&mut shell);

    let mut rl = rustyline::DefaultEditor::new().unwrap();

    let history_path = env::var("HOME")
        .map(|home| format!("{home}/.rc9_history"))
        .unwrap();

    if let Err(e) = rl.load_history(&history_path) {
        if !matches!(
            e,
            ReadlineError::Io(ref error)
                if error.kind() == std::io::ErrorKind::NotFound
        ) {
            eprintln!("rc9: failed to load history: {e}");
        }
    }

    loop {
        let dir = env::current_dir().unwrap();
        let prompt = format!("{}% ", dir.display());

        let mut input = match rl.readline(&prompt) {
            Ok(input) => input,

            Err(ReadlineError::Interrupted) => {
                continue;
            }

            Err(ReadlineError::Eof) => {
                break;
            }

            Err(e) => {
                eprintln!("rc9: {e}");
                break;
            }
        };

        if input.trim().is_empty() {
            continue;
        }

        let mut cancelled = false;

        while needs_more_input(&input) {
            match rl.readline("... ") {
                Ok(line) => {
                    input.push('\n');
                    input.push_str(&line);
                }

                Err(ReadlineError::Interrupted) => {
                    cancelled = true;
                    break;
                }

                Err(ReadlineError::Eof) => {
                    cancelled = true;
                    break;
                }

                Err(e) => {
                    eprintln!("rc9: {e}");
                    cancelled = true;
                    break;
                }
            }
        }

        if cancelled {
            continue;
        }

        if let Err(e) = rl.add_history_entry(input.as_str()) {
            eprintln!("rc9: failed to add history: {e}");
        }

        if input.trim() == "exit" {
            break;
        }

        execute_input(&mut shell, &input);
    }

    if let Err(e) = rl.save_history(&history_path) {
        eprintln!("rc9: failed to save history: {e}");
    }
}
