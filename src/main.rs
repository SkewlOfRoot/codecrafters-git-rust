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
    LsTree {
        object_id: String,

        #[arg(long, short)]
        name_only: bool,
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
            match result {
                Object::Blob(blob) => {
                    print!("{}", blob.content);
                }
                Object::Tree(tree) => {
                    print!("TODO");
                }
            }
            //println!("{:#?} - {}", result.object_type, result.content);
            //print!("{}", result.content);
            Ok(())
        }
        Commands::HashObject { file_path, write } => {
            let sha_hash = hash_object(file_path, write)?;
            print!("{}", sha_hash);
            Ok(())
        }
        Commands::LsTree {
            object_id,
            name_only,
        } => {
            ls_tree(object_id, name_only)?;
            Ok(())
        }
    }
}

fn cat_file(object_id: String) -> anyhow::Result<Object> {
    let git_object = load_git_object(object_id)?;
    Ok(git_object)
}

fn hash_object(file_path: String, write: bool) -> anyhow::Result<String> {
    let file_content = fs::read_to_string(file_path)?;
    let content_length = file_content.len();
    let object_content = format!("blob {}{}{}", content_length, '\0', file_content);

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

fn ls_tree(object_id: String, name_only: bool) -> anyhow::Result<()> {
    let tree_object = load_git_object(object_id)?;

    match tree_object {
        Object::Tree(tree) => {
            let names: Vec<String> = tree.elements.iter().map(|x| x.name.clone()).collect();
            println!("{}", names.join("\n"));
            Ok(())
        }
        _ => Err(anyhow!("Invalid object type.")),
    }
}

fn load_git_object(object_id: String) -> anyhow::Result<Object> {
    let folder: String = object_id.chars().take(2).collect();
    let file_name: String = object_id.chars().skip(2).collect();
    let object_path = format!("./.git/objects/{}/{}", folder, file_name);

    let file = fs::File::open(object_path)?;
    let reader = BufReader::new(file);

    let mut decoder = ZlibDecoder::new(reader);
    let mut buffer: Vec<u8> = Vec::new();
    decoder.read_to_end(&mut buffer)?;

    let parts: Vec<&[u8]> = buffer.split(|&byte| byte == 0x00).collect();
    let header = parts.first().expect("Zlib header not found.");
    let mut header_iter = header.split(|&byte| byte == 0x20);
    let object_type_bytes = header_iter.next().unwrap();
    let object_type = bytes_to_object_type(object_type_bytes)?;

    match object_type {
        ObjectType::Blob => Ok(Object::Blob(BlobObject::from_bytes(&buffer)?)),
        ObjectType::Tree => Ok(Object::Tree(TreeObject::from_bytes(&buffer)?)),
    }
}

struct BlobObject {
    length: u32,
    content: String,
}

struct TreeObject {
    length: u32,
    elements: Vec<TreeElement>,
}

#[derive(Debug)]
struct TreeElement {
    mode: String,
    object_type: ObjectType,
    hash: Vec<u8>,
    name: String,
}

impl BlobObject {
    fn from_bytes(input: &[u8]) -> anyhow::Result<BlobObject> {
        // Split input on null byte
        let parts: Vec<&[u8]> = input.split(|&byte| byte == 0x00).collect();

        let header = parts.first().expect("Zlib header not found.");
        // Split header on space
        let mut header_iter = header.split(|&byte| byte == 0x20);
        // Check if correct object type.
        if !bytes_to_object_type(header_iter.next().unwrap()).is_ok_and(|x| x == ObjectType::Blob) {
            return Err(anyhow!("Object is not of type Blob."));
        }
        // Extract length
        let length = header_iter.next().unwrap();
        let length: u32 = String::from_utf8(length.to_vec())?.parse::<u32>()?;

        // Extract content
        let content_bytes = parts.last().unwrap().to_vec();
        let content = String::from_utf8_lossy(&content_bytes);

        Ok(BlobObject {
            length,
            content: content.to_string(),
        })
    }
}

impl TreeObject {
    fn from_bytes(input: &[u8]) -> anyhow::Result<TreeObject> {
        // Read bytes until null byte.
        let (header_bytes, content_bytes) = match input.iter().position(|&byte| byte == 0) {
            Some(pos) => (&input[..pos], &input[pos + 1..]),
            None => (input, input),
        };

        // Split header on space
        let mut header_iter = header_bytes.split(|&byte| byte == 0x20);
        // Check if correct object type.
        if !bytes_to_object_type(header_iter.next().unwrap()).is_ok_and(|x| x == ObjectType::Tree) {
            return Err(anyhow!("Object is not of type Tree."));
        }
        // Extract length
        let length = header_iter.next().unwrap();
        let length: u32 = String::from_utf8(length.to_vec())?.parse::<u32>()?;

        //println!("content: {:#?}", String::from_utf8_lossy(content_bytes));
        //println!("content bytes: {:#?}", content_bytes);

        let mut elements: Vec<TreeElement> = Vec::new();

        let mut current_pos: usize = 0;
        let mut element_bytes: Vec<u8> = Vec::new();
        while current_pos < length.try_into().unwrap() {
            let byte = content_bytes[current_pos];

            if byte == 0 {
                let hash_bytes: &[u8] = &content_bytes[current_pos..current_pos + 1 + 20];
                element_bytes.extend_from_slice(hash_bytes);
                // println!(
                //     "element bytes: {:#?}",
                //     String::from_utf8_lossy(&element_bytes)
                // );
                elements.push(TreeElement::from_bytes(&element_bytes)?);
                current_pos += 21;
                element_bytes.clear();
            } else {
                element_bytes.push(byte);
                current_pos += 1;
            }
        }
        //println!("elements: {:#?}", elements);
        Ok(TreeObject { length, elements })
    }
}

impl TreeElement {
    fn from_bytes(input: &[u8]) -> anyhow::Result<TreeElement> {
        //println!("input element: {:#?}", input);
        // Read bytes until space.
        let mode_bytes = match input.iter().position(|&byte| byte == 32) {
            Some(pos) => &input[..pos],
            None => input,
        };

        let mode = String::from_utf8(mode_bytes.to_vec())?;
        //println!("mode: {:#?}", mode);

        let content_iter = input[mode_bytes.len() + 1..].iter();

        let mut name_bytes: Vec<u8> = Vec::new();
        for b in content_iter {
            if *b == 0 {
                break;
            }
            name_bytes.push(*b);
        }
        let name = String::from_utf8(name_bytes)?;
        //println!("name: {:#?}", name);

        let hash_begin_pos = mode_bytes.len() + 1 + name.len();
        let hash: Vec<u8> = input[hash_begin_pos..hash_begin_pos + 20].to_vec();
        //println!("hash: {:#?}", hex::encode(&hash));

        Ok(TreeElement {
            mode,
            object_type: ObjectType::Blob,
            hash,
            name,
        })
    }
}

fn bytes_to_object_type(object_type_bytes: &[u8]) -> Result<ObjectType, anyhow::Error> {
    let object_type = String::from_utf8(object_type_bytes.to_vec())?;

    let object_type = match object_type.as_str() {
        "blob" => ObjectType::Blob,
        "tree" => ObjectType::Tree,
        _ => return Err(anyhow!("Invalid object type.")),
    };

    Ok(object_type)
}

#[derive(Debug, PartialEq)]
enum ObjectType {
    Blob,
    Tree,
}

enum Object {
    Blob(BlobObject),
    Tree(TreeObject),
}
