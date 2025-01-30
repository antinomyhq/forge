mod return_ty;
mod supported_types;

use std::collections::HashMap;
use std::io::{self, Write, BufRead};

use crate::{return_ty::ReturnTy, supported_types::SupportedTypes};

pub fn shell(cmd: &str) -> anyhow::Result<()> {
    let vars = load_vars();
    let stdin = io::stdin().lock();
    print_prompt();
    println!("{}", cmd);
    io::stdout().flush().unwrap();
    if !exec(cmd, &vars) {
        return Ok(());
    }

    for line_result in stdin.lines() {
        match line_result {
            Ok(input) => {
                if !exec(&input, &vars) {
                    break;
                }
            }
            Err(_) => break,
        }

        // Print prompt before processing each line
        print_prompt();
    }

    Ok(())
}

fn exec(input: &str, vars: &HashMap<String, String>) -> bool {
    let input = input.trim();
    if input.is_empty() {
        return true;
    }

    let split = input.split(" ").collect::<Vec<_>>();

    if !split.is_empty() {
        let supported_types = SupportedTypes::from(split[0]);

        let return_ty = supported_types.eval(input, &vars);

        match return_ty {
            ReturnTy::Break => return false,
            ReturnTy::Continue => return true,
            ReturnTy::Expr(expr) => {
                if let Some(stderr) = expr.stderr {
                    eprintln!("{}", stderr.trim_end());
                } else if !expr.stdout.is_empty() {
                    println!("{}", expr.stdout.trim_end());
                }
            }
        }
    }

    true
}

fn load_vars() -> HashMap<String, String> {
    std::env::vars().collect()
}

fn print_prompt() {
    print!("$ ");
    io::stdout().flush().unwrap();
}
