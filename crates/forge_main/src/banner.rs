use std::io;

use colored::Colorize;

use crate::model::Command;

const BANNER: &str = include_str!("banner");

pub fn display() -> io::Result<()> {
    let commands = Command::available_commands();
    // Split the banner into lines and display each line dimmed
    println!("{} {}", BANNER.dimmed(), commands.join(", ").bold());
    Ok(())
}
