use crate::return_ty::{ExprTy, ReturnTy};
use std::{
    collections::HashMap, fs::OpenOptions, io::Write, path::PathBuf, process::Command, str::FromStr,
};

/// Here we can define all the supported commands
#[derive(Default, strum_macros::EnumString)]
pub enum SupportedTypes {
    #[strum(ascii_case_insensitive)]
    Exit,
    #[strum(ascii_case_insensitive)]
    Echo,
    #[strum(ascii_case_insensitive)]
    Type,
    #[strum(ascii_case_insensitive)]
    Pwd,
    #[strum(ascii_case_insensitive)]
    Cd,

    #[default]
    #[strum(disabled)]
    Default,
}

#[derive(Debug)]
pub struct Seperators {
    seps: Vec<Seperator>,
}

#[allow(unused)]
#[derive(Debug)]
pub enum Seperator {
    Expr(Vec<String>),
    Redirect {
        fd: Option<u32>,
        operator: String,
        target: String,
    },
    Pipe,
    And,
    Or,
    Seq,
}

impl Seperators {
    pub fn new(input: &str) -> Self {
        let tokens = parse_quote(input);
        let mut seps = vec![];
        let mut current_expr = vec![];

        let mut iter = tokens.into_iter().peekable();

        while let Some(token) = iter.next() {
            // Match for file descriptor redirections like "1>" or "2>>"
            if let Some(operator) = token.strip_prefix(|c: char| c.is_numeric()) {
                if operator == ">" || operator == ">>" {
                    if !current_expr.is_empty() {
                        seps.push(Seperator::Expr(current_expr.clone()));
                        current_expr.clear();
                    }
                    let fd = token
                        .chars()
                        .take_while(|c| c.is_numeric())
                        .collect::<String>();
                    if let Ok(fd_num) = fd.parse::<u32>() {
                        if let Some(target) = iter.next() {
                            seps.push(Seperator::Redirect {
                                fd: Some(fd_num),
                                operator: operator.to_string(),
                                target,
                            });
                        }
                    }
                    continue;
                }
            }

            // Match for simple redirections like ">" or ">>"
            if token == ">" || token == ">>" {
                if !current_expr.is_empty() {
                    seps.push(Seperator::Expr(current_expr.clone()));
                    current_expr.clear();
                }
                if let Some(target) = iter.next() {
                    seps.push(Seperator::Redirect {
                        fd: None,
                        operator: token,
                        target,
                    });
                }
                continue;
            }

            // Match logical and pipe operators
            if token == "&&" || token == "||" || token == ";" || token == "|" {
                if !current_expr.is_empty() {
                    seps.push(Seperator::Expr(current_expr.clone()));
                    current_expr.clear();
                }
                let sep = match token.as_str() {
                    "&&" => Seperator::And,
                    "||" => Seperator::Or,
                    ";" => Seperator::Seq,
                    "|" => Seperator::Pipe,
                    _ => unreachable!(),
                };
                seps.push(sep);
                continue;
            }

            // Add token to the current expression if it doesn't match other patterns
            current_expr.push(token);
        }

        // Add the remaining expression, if any
        if !current_expr.is_empty() {
            seps.push(Seperator::Expr(current_expr.clone()));
        }

        Seperators { seps }
    }
}

impl SupportedTypes {
    pub fn from<T: AsRef<str>>(v: T) -> Self {
        Self::from_str(v.as_ref()).unwrap_or(Self::Default)
    }
}

impl SupportedTypes {
    // Evaluating the enum give much more type safety
    // over matching a string.
    pub fn eval<T: AsRef<str>>(self, input: T, vars: &HashMap<String, String>) -> ReturnTy {
        let seps = Seperators::new(input.as_ref());
        // println!("{:?}", seps);
        let iter = seps.seps.into_iter();

        let mut final_ret = ReturnTy::Continue;

        for cur_sep in iter {
            match cur_sep {
                Seperator::Expr(expr) => {
                    let ret = self.eval_single(expr, vars);
                    final_ret = ret;
                }
                Seperator::Redirect {
                    fd,
                    operator,
                    target,
                } => {
                    let path = PathBuf::from(target.clone());
                    let _ = std::fs::create_dir_all(
                        path.parent()
                            .map(|v| v.to_path_buf())
                            .unwrap_or(PathBuf::from(target.clone())),
                    );

                    let file = match operator.as_str() {
                        ">" => OpenOptions::new()
                            .write(true)
                            .truncate(true)
                            .create(true)
                            .open(path),
                        ">>" => OpenOptions::new()
                            .truncate(false)
                            .create(true)
                            .append(true)
                            .open(path),
                        "<" => OpenOptions::new().read(true).open(path),
                        _ => Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Invalid operator",
                        )),
                    };

                    match file {
                        Ok(mut file) => {
                            if let ReturnTy::Expr(output) = &mut final_ret {
                                match fd {
                                    Some(1) | None => {
                                        if let Some(err) = &output.stderr {
                                            eprintln!("{}", err.trim_end());
                                            output.stderr = None;
                                        }
                                        let mut out = output.as_string().to_string();
                                        if !out.is_empty() {
                                            out = format!("{}\n", out.trim());
                                        }
                                        file.write_all(out.as_bytes()).unwrap();
                                        final_ret = ReturnTy::Continue;
                                    }
                                    Some(2) => {
                                        if !output.stdout.is_empty() {
                                            println!("{}", output.stdout.trim_end());
                                            output.stdout = String::new();
                                        }
                                        if let Some(stderr) = &output.stderr {
                                            let mut out = stderr.to_string();
                                            if !out.is_empty() {
                                                out = format!("{}\n", out.trim());
                                            }

                                            file.write_all(out.as_bytes()).unwrap();
                                            output.stderr = None;
                                        }
                                        final_ret = ReturnTy::Continue;
                                    }
                                    _ => {
                                        // idk what to do here
                                        todo!()
                                    }
                                }
                            }
                        }
                        Err(_e) => {
                            // idk what to do here
                            println!("Error opening file: {}", _e);
                        }
                    }
                }
                Seperator::Pipe => todo!(),
                Seperator::And => todo!(),
                Seperator::Or => todo!(),
                Seperator::Seq => todo!(),
            }
        }
        final_ret
    }

    fn eval_single<T: AsRef<str>>(
        &self,
        split: Vec<T>,
        vars: &HashMap<String, String>,
    ) -> ReturnTy {
        match self {
            Self::Exit => handle_exit(),
            Self::Echo => handle_echo(split),
            Self::Type => handle_type(split, vars),
            Self::Pwd => handle_pwd(),
            Self::Cd => handle_cd(split),
            Self::Default => handle_default(split),
        }
    }
}

fn parse_quote<T: AsRef<str>>(input: T) -> Vec<String> {
    let mut stack = vec![];
    let mut inside_quotes = false;
    let mut inside_double_quotes = false;
    let mut current_segment = String::new();

    let mut iter = input.as_ref().chars().peekable();

    while let Some(c) = iter.next() {
        match c {
            '\'' => {
                if !inside_double_quotes {
                    inside_quotes = !inside_quotes;
                } else {
                    current_segment.push(c);
                }
            }
            '"' => {
                if !inside_quotes {
                    inside_double_quotes = !inside_double_quotes;
                } else {
                    current_segment.push(c);
                }
            }

            '\\' => {
                if !inside_quotes && !inside_double_quotes {
                    if let Some(next) = iter.next() {
                        current_segment.push(next);
                    }
                } else if inside_double_quotes {
                    if let Some(&next) = iter.peek() {
                        match next {
                            '\\' | '$' | '"' => {
                                current_segment.push(iter.next().unwrap());
                            }
                            _ => current_segment.push(c),
                        }
                    }
                } else {
                    current_segment.push(c);
                }
            }

            ' ' => {
                if !inside_quotes && !inside_double_quotes {
                    if !current_segment.is_empty() {
                        stack.push(current_segment);
                        current_segment = String::new();
                    }
                } else {
                    current_segment.push(c);
                }
            }

            c => current_segment.push(c),
        }
    }

    if !current_segment.is_empty() {
        stack.push(current_segment);
    }

    stack
}

#[allow(deprecated)]
fn normalize_path(path: &str) -> String {
    match path {
        "~" => std::env::home_dir()
            .and_then(|v| v.to_str().map(String::from))
            .unwrap_or_default(),
        x => x.to_string(),
    }
}

fn handle_cd<T: AsRef<str>>(split: Vec<T>) -> ReturnTy {
    if split.len() > 1 {
        let path = split[1].as_ref();
        let path = normalize_path(path);
        if PathBuf::from(&path)
            .canonicalize()
            .ok()
            .and_then(|absolute| std::env::set_current_dir(absolute).ok())
            .is_none()
        {
            return ReturnTy::Expr(
                ExprTy::default().stderr(format!("cd: {}: No such file or directory", path)),
            );
        }
    }
    ReturnTy::Continue
}

fn handle_pwd() -> ReturnTy {
    ReturnTy::Expr(
        ExprTy::default().stdout(
            std::env::current_dir()
                .ok()
                .and_then(|v| v.to_str().map(String::from))
                .unwrap_or_default(),
        ),
    )
}

fn handle_path_ty(prog: &str, vars: &HashMap<String, String>) -> Option<String> {
    if let Some(path_env_var) = vars.get("PATH") {
        let paths = path_env_var.split(":");
        let paths = paths.collect::<Vec<_>>();

        let val = paths.iter().find(|path| {
            std::fs::read_dir(path)
                .ok()
                .and_then(|mut read_dir| {
                    read_dir.find_map(|dir_entry_result| {
                        dir_entry_result
                            .map(|dir_entry| dir_entry.file_name().eq(prog))
                            .ok()
                            .and_then(|v| if v { Some(true) } else { None })
                    })
                })
                .unwrap_or_default()
        });
        if let Some(path) = val {
            return Some(format!("{} is {}/{}", prog, path, prog));
        }
    }

    None
}

fn populate_args<T: AsRef<str>>(mut command: Command, split: &[T]) -> Command {
    if split.len() > 1 {
        let args = (split[1..])
            .iter()
            .map(|v| v.as_ref())
            .map(String::from)
            .collect::<Vec<_>>();

        command.args(args);
    }
    command
}

fn execute_cmd<T: AsRef<str>>(split: Vec<T>) -> ExprTy {
    let command: &str = split[0].as_ref();
    let mut cmd = populate_args(Command::new(command), &split);

    let output = cmd.output();
    match output {
        Ok(v) => {
            let mut expr = ExprTy::default().stdout(String::from_utf8(v.stdout).unwrap());
            if !v.stderr.is_empty() {
                expr = expr.stderr(String::from_utf8(v.stderr).unwrap());
            }

            expr
        }
        Err(_e) => ExprTy::default().stderr(format!("{}: command not found\n", command)),
    }
}

fn handle_type<T: AsRef<str>>(split: Vec<T>, vars: &HashMap<String, String>) -> ReturnTy {
    if split.len() > 1 {
        let prog = split[1].as_ref();
        let expr = match SupportedTypes::from(prog) {
            SupportedTypes::Default => {
                if let Some(path) = handle_path_ty(prog, vars) {
                    ExprTy::default().stdout(path.to_string())
                } else {
                    ExprTy::default().stderr(format!("{}: not found", prog))
                }
            }
            _ => ExprTy::default().stdout(format!("{} is a shell builtin", prog)),
        };

        return ReturnTy::Expr(expr);
    }
    ReturnTy::Continue
}

fn handle_default<T: AsRef<str>>(split: Vec<T>) -> ReturnTy {
    if !split.is_empty() {
        let res = execute_cmd(split);
        return ReturnTy::Expr(res);
    }

    ReturnTy::Continue
}

fn handle_echo<T: AsRef<str>>(ans: Vec<T>) -> ReturnTy {
    ReturnTy::Expr(
        ExprTy::default().stdout(
            ans.iter()
                .skip(1)
                .map(|v| v.as_ref())
                .map(String::from)
                .collect::<Vec<_>>()
                .join(" "),
        ),
    )
}

fn handle_exit() -> ReturnTy {
    ReturnTy::Break
}
