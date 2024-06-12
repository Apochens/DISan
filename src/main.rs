mod instrument;
mod matcher;
mod traverse;
mod ast;
mod edit;

use std::{fs, path::{Path, PathBuf}, process::exit};
use clap::Parser;

use colored::Colorize;
use edit::EditConstant;
use instrument::Instrumenter;

#[derive(Parser)]
#[command(name = "DISan")]
struct DISan {
    target: String,
}

fn check_code(buf: &str) -> bool {
    buf.contains(&EditConstant::header_include_str()) &&
    buf.contains(&EditConstant::global_var_decl_str()) 
}

fn write_code(contents: &str, file_name: &str) {
    let path = "./instrumented/".to_string() + file_name;
    
    fs::write(path, contents).unwrap();
}

fn instrument_code(path: &PathBuf) {
    let absolute_path = path.canonicalize().unwrap();
    let mut code = fs::read_to_string(&path).unwrap();
    let file_name = path.file_name().unwrap().to_str().unwrap().to_string();

    let mut instrumenter = Instrumenter::new(file_name.to_owned());
    instrumenter.instrument(&mut code);

    let file_str = absolute_path.to_str().unwrap();
    if check_code(&code) {
        write_code(&code, &file_name);
        println!("{} ({})", "Finished the instrumentation!".green().bold(), &file_str);
        return ;
    } else {
        eprintln!("{} ({})", "Failed the instrumentation check!".red().bold(), &file_str);
        exit(-1);
    }
}

fn main() {
    let disan = DISan::parse();
    let path = Path::new(&disan.target);
    if !path.exists() {
        eprintln!("{} does not exist!", &disan.target);
        return ;
    }

    let mut work_list: Vec<PathBuf> = vec![];

    if path.is_file() && path.extension().unwrap() == "cpp" {
        work_list.push(path.to_path_buf());
    }

    if path.is_dir() {
        for entry in path.read_dir().expect("Failed to read the dir") {
            if let Ok(e) = entry {
                let file_path = e.path();
                if file_path.is_file() && file_path.extension().unwrap() == "cpp" {
                    work_list.push(file_path);
                }
            }
        }
    }

    if work_list.is_empty() {
        println!("No file to instrument. Exit.");
        return ;
    }

    let output_dir = Path::new("./instrumented/");
    if !output_dir.is_dir() {
        match fs::create_dir(output_dir) {
            Ok(()) => println!("Create output directory: {}", output_dir.to_str().unwrap()),
            Err(e) => eprintln!("Failed to create output directory: {}", e),
        };
    }

    work_list.iter().for_each(instrument_code);

    if output_dir.read_dir().expect("Error").count() == 0 {
        fs::remove_dir(output_dir).unwrap();
    }
}
