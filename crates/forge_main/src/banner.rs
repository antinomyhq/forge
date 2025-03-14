use std::io;

use colored::Colorize;

const BANNER: &str = include_str!("banner");

pub fn display(commands: Vec<String>) -> io::Result<()> {
    // Split the banner into lines and display each line dimmed
    println!("{} {}", BANNER.dimmed(), commands.join(", ").bold());
    Ok(())
}
