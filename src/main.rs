use anyhow::{anyhow, Context, Ok};
use clap::{Parser, Subcommand};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use sha1::{Digest, Sha1};
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::{env, fs};

#[derive(Parser)]
#[clap(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[arg(short, long)]
    pretty_print: Option<bool>,

    #[command(subcommand)]
    commands: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    CatFile {
        object_id: String,

        #[arg(long, short)]
        pretty: bool,
    },
    HashObject {
        file_path: String,

        #[arg(long, short)]
        write: bool,
    },
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
        Commands::CatFile { object_id, pretty } => {
            let result = cat_file(object_id)?;
            print!("{}", result.content);
            Ok(())
        }
        Commands::HashObject { file_path, write } => {
            let sha_hash = hash_object(file_path, write)?;
            print!("{}", sha_hash);
            Ok(())
        }
    }
}

fn cat_file(object_id: String) -> anyhow::Result<GitObject> {
    let folder: String = object_id.chars().take(2).collect();
    let file_name: String = object_id.chars().skip(2).collect();
    let object_path = format!("./.git/objects/{}/{}", folder, file_name);

    let content = load_object_content(object_path)?;
    Ok(GitObject::from_bytes(&content)?)
}

fn load_object_content(object_path: String) -> Result<Vec<u8>, anyhow::Error> {
    let file = fs::File::open(object_path)?;
    let reader = BufReader::new(file);

    let mut decoder = ZlibDecoder::new(reader);
    let mut buffer: Vec<u8> = Vec::new();
    decoder.read_to_end(&mut buffer)?;

    Ok(buffer)
}

fn hash_object(file_path: String, write: bool) -> anyhow::Result<String> {
    let file_content = fs::read_to_string(file_path)?;
    let content_length = file_content.len();
    let object_content = format!("blob {}{}{}", content_length, 0x00, file_content);

    let sha_hash = calculate_sha_hash(&object_content);
    let sha_hash = hex::encode(sha_hash);

    if write {
        let zlib_content = zlib_compress(&object_content)?;
        let folder: String = sha_hash.chars().take(2).collect();
        let object_file_name: String = sha_hash.chars().skip(2).collect();
        let full_path = format!("./.git/objects/{}/{}", folder, object_file_name);
        let full_path = Path::new(&full_path);

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(full_path, zlib_content).context("Write object file.")?
    }

    Ok(sha_hash)
}

fn zlib_compress(object_content: &str) -> anyhow::Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(object_content.as_bytes())?;
    let compressed = encoder.finish()?;
    Ok(compressed)
}

fn calculate_sha_hash(object_content: &str) -> Vec<u8> {
    let mut hasher = Sha1::new();
    hasher.update(object_content.as_bytes());
    let result = hasher.finalize();

    result[..].to_vec()
}

#[derive(Debug)]
struct GitObject {
    object_type: ObjectType,
    length: u32,
    content: String,
}

impl GitObject {
    fn from_bytes(input: &[u8]) -> anyhow::Result<GitObject> {
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
enum ObjectType {
    Blob,
    Tree,
    Commit,
}
