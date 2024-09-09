use anyhow::{anyhow, Ok};
use clap::{Args, Parser, Subcommand};
use flate2::read::ZlibDecoder;
use std::io::{BufReader, Read};
use std::{env, fs};

#[derive(Parser)]
#[clap(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    commands: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    #[command(name = "cat-file")]
    CatFile(CatFileArgs),
}

#[derive(Args)]
struct CatFileArgs {
    #[arg(short, long)]
    pretty_print: Option<bool>,
    object: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.commands {
        Commands::Init => {
            let cwd = env::current_dir()?;
            println!("Initializing git in {:#?}", cwd);
            fs::create_dir(".git").unwrap();
            fs::create_dir(".git/objects").unwrap();
            fs::create_dir(".git/refs").unwrap();
            fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
            println!("Initialized git directory");
            Ok(())
        }
        Commands::CatFile(args) => {
            let result = cat_file(args.object)?;
            print!("{}", result.content);
            Ok(())
        }
    }
}

fn cat_file(object_path: String) -> anyhow::Result<GitObject> {
    let content = load_object_content(object_path)?;
    Ok(GitObject::new(&content)?)
}

fn load_object_content(object_path: String) -> Result<Vec<u8>, anyhow::Error> {
    let file = fs::File::open(object_path)?;
    let reader = BufReader::new(file);

    let mut decoder = ZlibDecoder::new(reader);
    let mut buffer: Vec<u8> = Vec::new();
    decoder.read_to_end(&mut buffer)?;

    Ok(buffer)
}

fn to_object_type(object_type_bytes: &[u8]) -> Result<ObjectType, anyhow::Error> {
    let object_type = String::from_utf8(object_type_bytes.to_vec())?;

    let object_type = match object_type.as_str() {
        "blob" => ObjectType::Blob,
        "tree" => ObjectType::Tree,
        "commit" => ObjectType::Commit,
        _ => return Err(anyhow!("Invalid object type.")),
    };

    Ok(object_type)
}

#[derive(Debug)]
struct GitObject {
    object_type: ObjectType,
    length: u32,
    content: String,
}

impl GitObject {
    fn new(input: &[u8]) -> anyhow::Result<GitObject> {
        // Split on null byte
        let parts: Vec<&[u8]> = input.split(|&byte| byte == 0x00).collect();
        let content = parts.last().unwrap();
        let content = String::from_utf8(content.to_vec())?;
        let header = parts.first().expect("Zlib header not found.");

        // Split on space
        let mut header_iter = header.split(|&byte| byte == 0x20);
        let object_type_bytes = header_iter.next().unwrap();
        let object_type = to_object_type(object_type_bytes)?;

        let length = header_iter.next().unwrap();
        let length: u32 = String::from_utf8(length.to_vec())?.parse::<u32>()?;

        Ok(GitObject {
            object_type,
            length,
            content,
        })
    }
}

#[derive(Debug)]
enum ObjectType {
    Blob,
    Tree,
    Commit,
}
